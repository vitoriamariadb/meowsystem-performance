// O que se lê do kernel sem pedir nada a ninguém: a placa NVIDIA no sysfs, o
// perfil da placa-mãe (platform_profile), o id do boot. Só leitura: nada aqui
// escreve, e o nvidia-smi só roda com a placa acordada.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::modelo::{CONSULTA_NVIDIA, EstadoGpu, ler_nvidia_smi};

/// O nvidia-smi preso não pode prender o applet.
const PRAZO_NVIDIA_SMI: Duration = Duration::from_secs(3);

/// A placa NVIDIA (vendor 0x10de, classe de vídeo 0x03xxxx) no sysfs.
fn placa_nvidia(sys: &Path) -> Option<PathBuf> {
    let dir = std::fs::read_dir(sys.join("bus/pci/devices")).ok()?;
    dir.flatten().map(|e| e.path()).find(|p| {
        let ler = |f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default();
        ler("vendor").trim() == "0x10de" && ler("class").trim().starts_with("0x03")
    })
}

pub fn modulo_nvidia_carregado(sys: &Path) -> bool {
    sys.join("module/nvidia").is_dir()
}

/// O estado da NVIDIA. `rodar_smi` só é chamado com a placa acordada: o
/// nvidia-smi tiraria a placa da suspensão só para dizer que ela dormia.
pub fn sondar_gpu(sys: &Path, rodar_smi: impl FnOnce() -> Option<String>) -> EstadoGpu {
    let Some(placa) = placa_nvidia(sys) else {
        return EstadoGpu::Desligada;
    };
    if !modulo_nvidia_carregado(sys) {
        return EstadoGpu::Desligada;
    }
    let energia = std::fs::read_to_string(placa.join("power/runtime_status")).unwrap_or_default();
    if energia.trim() == "suspended" {
        return EstadoGpu::Dormindo;
    }
    match rodar_smi().as_deref().and_then(ler_nvidia_smi) {
        Some(l) => EstadoGpu::Ativa(l),
        None => EstadoGpu::SemResposta,
    }
}

/// `nvidia-smi --query-gpu=...`, com prazo. `None` em qualquer falha.
pub fn nvidia_smi() -> Option<String> {
    let mut filho = Command::new("nvidia-smi")
        .arg(format!("--query-gpu={CONSULTA_NVIDIA}"))
        .arg("--format=csv,noheader,nounits")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let inicio = Instant::now();
    loop {
        match filho.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if inicio.elapsed() > PRAZO_NVIDIA_SMI => {
                let _ = filho.kill();
                let _ = filho.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut saida = String::new();
    std::io::Read::read_to_string(&mut filho.stdout.take()?, &mut saida).ok()?;
    Some(saida)
}

/// O perfil da placa-mãe, preferindo o do acer-wmi quando há mais de um.
pub fn perfil_da_placa(sys: &Path) -> Option<String> {
    let dir = std::fs::read_dir(sys.join("class/platform-profile")).ok()?;
    let mut perfis: Vec<(String, String)> = dir
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let nome = std::fs::read_to_string(p.join("name")).ok()?;
            let perfil = std::fs::read_to_string(p.join("profile")).ok()?;
            Some((nome.trim().to_string(), perfil.trim().to_string()))
        })
        .collect();
    perfis.sort_by_key(|(nome, _)| nome != "acer-wmi");
    perfis.into_iter().next().map(|(_, perfil)| perfil)
}

pub fn id_do_boot() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::fs;

    struct SysFalso(PathBuf);

    impl SysFalso {
        fn novo(nome: &str) -> Self {
            let raiz = std::env::temp_dir().join(format!("meowsystem-performance-{}-{nome}", std::process::id()));
            let _ = fs::remove_dir_all(&raiz);
            fs::create_dir_all(raiz.join("bus/pci/devices")).unwrap();
            Self(raiz)
        }

        fn arquivo(&self, rel: &str, conteudo: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, conteudo).unwrap();
        }

        fn placa(&self, energia: &str) {
            self.arquivo("bus/pci/devices/0000:00:02.0/vendor", "0x8086\n");
            self.arquivo("bus/pci/devices/0000:00:02.0/class", "0x030000\n");
            self.arquivo("bus/pci/devices/0000:01:00.1/vendor", "0x10de\n");
            self.arquivo("bus/pci/devices/0000:01:00.1/class", "0x040300\n");
            self.arquivo("bus/pci/devices/0000:01:00.0/vendor", "0x10de\n");
            self.arquivo("bus/pci/devices/0000:01:00.0/class", "0x030000\n");
            self.arquivo("bus/pci/devices/0000:01:00.0/power/runtime_status", energia);
        }
    }

    impl Drop for SysFalso {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const SMI: &str = "NVIDIA GeForce RTX 4060 Laptop GPU, 47, 1.71, 0, Enabled, P8\n";

    #[test]
    fn placa_dormindo_nao_e_acordada() {
        let s = SysFalso::novo("dormindo");
        s.placa("suspended\n");
        fs::create_dir_all(s.0.join("module/nvidia")).unwrap();
        let estado = sondar_gpu(&s.0, || panic!("o nvidia-smi acordaria a placa"));
        assert_eq!(estado, EstadoGpu::Dormindo);
    }

    #[test]
    fn placa_acordada_e_lida_pelo_nvidia_smi() {
        let s = SysFalso::novo("ativa");
        s.placa("active\n");
        fs::create_dir_all(s.0.join("module/nvidia")).unwrap();
        match sondar_gpu(&s.0, || Some(SMI.into())) {
            EstadoGpu::Ativa(l) => assert_eq!(l.persistencia, Some(true)),
            outro => panic!("{outro:?}"),
        }
        assert_eq!(sondar_gpu(&s.0, || None), EstadoGpu::SemResposta);
    }

    #[test]
    fn sem_modulo_ou_sem_placa_e_desligada() {
        let s = SysFalso::novo("sem-modulo");
        assert_eq!(sondar_gpu(&s.0, || panic!()), EstadoGpu::Desligada);
        s.placa("active\n");
        assert!(!modulo_nvidia_carregado(&s.0));
        assert_eq!(sondar_gpu(&s.0, || panic!()), EstadoGpu::Desligada);
    }

    #[test]
    fn perfil_da_placa_prefere_o_acer() {
        let s = SysFalso::novo("perfil");
        assert_eq!(perfil_da_placa(&s.0), None);
        s.arquivo("class/platform-profile/platform-profile-0/name", "outro\n");
        s.arquivo("class/platform-profile/platform-profile-0/profile", "balanced\n");
        s.arquivo("class/platform-profile/platform-profile-1/name", "acer-wmi\n");
        s.arquivo("class/platform-profile/platform-profile-1/profile", "balanced-performance\n");
        assert_eq!(perfil_da_placa(&s.0).as_deref(), Some("balanced-performance"));
    }
}
