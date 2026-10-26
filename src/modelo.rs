// O que o applet sabe do sistema, sem saber de onde veio: os nomes do
// system76-power, a leitura do nvidia-smi e a regra do reinício pendente.
// Nada aqui fala D-Bus nem abre arquivo, e é por isso que tudo aqui tem teste.

/// Os três perfis do system76-power, com os nomes do D-Bus dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perfil {
    Economia,
    Equilibrado,
    Desempenho,
}

impl Perfil {
    pub const TODOS: [Perfil; 3] = [Perfil::Desempenho, Perfil::Equilibrado, Perfil::Economia];

    /// O que o `GetProfile` devolve.
    pub fn do_daemon(v: &str) -> Option<Self> {
        match v.trim() {
            "Battery" => Some(Self::Economia),
            "Balanced" => Some(Self::Equilibrado),
            "Performance" => Some(Self::Desempenho),
            _ => None,
        }
    }

    /// O método que troca para este perfil (sem argumento, um por perfil).
    pub fn metodo(self) -> &'static str {
        match self {
            Self::Economia => "Battery",
            Self::Equilibrado => "Balanced",
            Self::Desempenho => "Performance",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Self::Economia => "Economia",
            Self::Equilibrado => "Equilibrado",
            Self::Desempenho => "Desempenho",
        }
    }
}

/// O modo gráfico, com os nomes do `GetGraphics`/`SetGraphics`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModoGrafico {
    Integrada,
    Hibrida,
    Nvidia,
    /// A NVIDIA só para computação. O applet mostra, mas não oferece.
    Computacao,
}

impl ModoGrafico {
    /// Os que o popup oferece.
    pub const OFERECIDOS: [ModoGrafico; 3] = [ModoGrafico::Integrada, ModoGrafico::Hibrida, ModoGrafico::Nvidia];

    pub fn do_daemon(v: &str) -> Option<Self> {
        match v.trim() {
            "integrated" => Some(Self::Integrada),
            "hybrid" => Some(Self::Hibrida),
            "nvidia" => Some(Self::Nvidia),
            "compute" => Some(Self::Computacao),
            _ => None,
        }
    }

    pub fn para_daemon(self) -> &'static str {
        match self {
            Self::Integrada => "integrated",
            Self::Hibrida => "hybrid",
            Self::Nvidia => "nvidia",
            Self::Computacao => "compute",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Self::Integrada => "Integrada",
            Self::Hibrida => "Híbrida",
            Self::Nvidia => "NVIDIA",
            Self::Computacao => "Computação",
        }
    }
}

/// O perfil da placa (o `platform_profile` do acer-wmi), que o `nitro-profiled`
/// acompanha a partir do perfil do system76-power.
pub fn nome_do_perfil_da_placa(v: &str) -> String {
    match v.trim() {
        "low-power" => "Economia".into(),
        "quiet" => "Silencioso".into(),
        "balanced" => "Equilibrado".into(),
        "balanced-performance" => "Equilibrado+".into(),
        "performance" => "Desempenho".into(),
        outro => outro.into(),
    }
}

/// O modo gráfico que vale desde o boot, e se é preciso gravá-lo agora.
///
/// O system76-power responde o modo CONFIGURADO, que só vale depois de
/// reiniciar. O primeiro modo visto em cada boot é a fotografia do que está
/// rodando; um configurado diferente dela é reinício pendente.
pub fn fotografia_do_boot(
    gravada: Option<(&str, ModoGrafico)>,
    boot_atual: &str,
    configurado: ModoGrafico,
) -> (ModoGrafico, bool) {
    match gravada {
        Some((boot, modo)) if boot == boot_atual => (modo, false),
        _ => (configurado, true),
    }
}

/// Trocar o modo gráfico só vale depois de reiniciar: está pendente quando o
/// configurado não é o do boot, ou quando o configurado é a integrada e o
/// módulo da NVIDIA continua carregado.
pub fn reinicio_pendente(configurado: ModoGrafico, no_boot: ModoGrafico, modulo_nvidia: bool) -> bool {
    configurado != no_boot || (configurado == ModoGrafico::Integrada && modulo_nvidia)
}

/// O que se sabe da NVIDIA agora.
#[derive(Clone, Debug, PartialEq)]
pub enum EstadoGpu {
    /// Sem placa NVIDIA, ou sem o módulo carregado.
    Desligada,
    /// A placa está em suspensão de energia: perguntar ao nvidia-smi a acordaria.
    Dormindo,
    Ativa(LeituraGpu),
    /// O módulo está carregado e o nvidia-smi não respondeu.
    SemResposta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeituraGpu {
    pub nome: String,
    pub temperatura_c: Option<u32>,
    pub consumo_w: Option<f32>,
    pub uso_pct: Option<u32>,
    pub persistencia: Option<bool>,
    pub pstate: Option<String>,
}

/// As colunas pedidas ao nvidia-smi, na ordem que o `ler_nvidia_smi` espera.
pub const CONSULTA_NVIDIA: &str = "name,temperature.gpu,power.draw,utilization.gpu,persistence_mode,pstate";

/// A primeira linha de `nvidia-smi --query-gpu=<CONSULTA_NVIDIA>
/// --format=csv,noheader,nounits`. `[N/A]` e lixo viram `None`.
pub fn ler_nvidia_smi(saida: &str) -> Option<LeituraGpu> {
    let linha = saida.lines().map(str::trim).find(|l| !l.is_empty())?;
    let c: Vec<&str> = linha.split(',').map(str::trim).collect();
    if c.len() < 6 || c[0].is_empty() {
        return None;
    }
    let numero = |v: &str| v.parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0);
    Some(LeituraGpu {
        nome: c[0].to_string(),
        temperatura_c: numero(c[1]).map(|n| n.round() as u32),
        consumo_w: numero(c[2]),
        uso_pct: numero(c[3]).map(|n| n.round() as u32),
        persistencia: match c[4] {
            "Enabled" => Some(true),
            "Disabled" => Some(false),
            _ => None,
        },
        pstate: (c[5].starts_with('P')).then(|| c[5].to_string()),
    })
}

/// A linha curta da GPU: "47 °C · 1,7 W · 0% · P8". Vírgula decimal, como no
/// resto da interface em português.
pub fn resumo_da_gpu(l: &LeituraGpu) -> String {
    let mut partes = Vec::new();
    if let Some(t) = l.temperatura_c {
        partes.push(format!("{t} °C"));
    }
    if let Some(w) = l.consumo_w {
        partes.push(format!("{w:.1} W").replace('.', ","));
    }
    if let Some(u) = l.uso_pct {
        partes.push(format!("{u}%"));
    }
    if let Some(p) = &l.pstate {
        partes.push(p.clone());
    }
    partes.join(" · ")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn perfis_vao_e_voltam_com_os_nomes_do_daemon() {
        for p in Perfil::TODOS {
            assert_eq!(Perfil::do_daemon(p.metodo()), Some(p));
        }
        assert_eq!(Perfil::do_daemon("Battery\n"), Some(Perfil::Economia));
        assert_eq!(Perfil::do_daemon("Turbo"), None);
    }

    #[test]
    fn modos_vao_e_voltam_com_os_nomes_do_daemon() {
        for m in [ModoGrafico::Integrada, ModoGrafico::Hibrida, ModoGrafico::Nvidia, ModoGrafico::Computacao] {
            assert_eq!(ModoGrafico::do_daemon(m.para_daemon()), Some(m));
        }
        assert_eq!(ModoGrafico::do_daemon("NVIDIA"), None);
        assert!(!ModoGrafico::OFERECIDOS.contains(&ModoGrafico::Computacao));
    }

    #[test]
    fn perfil_da_placa_em_portugues() {
        assert_eq!(nome_do_perfil_da_placa("balanced-performance\n"), "Equilibrado+");
        assert_eq!(nome_do_perfil_da_placa("low-power"), "Economia");
        assert_eq!(nome_do_perfil_da_placa("custom"), "custom");
    }

    #[test]
    fn a_fotografia_so_e_gravada_uma_vez_por_boot() {
        let n = ModoGrafico::Nvidia;
        let h = ModoGrafico::Hibrida;
        assert_eq!(fotografia_do_boot(None, "b1", n), (n, true));
        assert_eq!(fotografia_do_boot(Some(("b0", h)), "b1", n), (n, true));
        assert_eq!(fotografia_do_boot(Some(("b1", h)), "b1", n), (h, false));
    }

    #[test]
    fn reinicio_pendente_quando_o_configurado_nao_e_o_do_boot() {
        use ModoGrafico::*;
        assert!(!reinicio_pendente(Nvidia, Nvidia, true));
        assert!(reinicio_pendente(Hibrida, Nvidia, true));
        assert!(reinicio_pendente(Integrada, Integrada, true));
        assert!(!reinicio_pendente(Integrada, Integrada, false));
        // Sem módulo com a NVIDIA configurada é driver que não carregou, não reinício.
        assert!(!reinicio_pendente(Nvidia, Nvidia, false));
    }

    #[test]
    fn nvidia_smi_desta_maquina() {
        let l = ler_nvidia_smi("NVIDIA GeForce RTX 4060 Laptop GPU, 47, 1.71, 0, Enabled, P8\n").unwrap();
        assert_eq!(l.nome, "NVIDIA GeForce RTX 4060 Laptop GPU");
        assert_eq!(l.temperatura_c, Some(47));
        assert_eq!(l.consumo_w, Some(1.71));
        assert_eq!(l.uso_pct, Some(0));
        assert_eq!(l.persistencia, Some(true));
        assert_eq!(l.pstate.as_deref(), Some("P8"));
        assert_eq!(resumo_da_gpu(&l), "47 °C · 1,7 W · 0% · P8");
    }

    #[test]
    fn nvidia_smi_com_campos_ausentes() {
        let l = ler_nvidia_smi("Quadro, [N/A], [N/A], [N/A], Disabled, [N/A]").unwrap();
        assert_eq!(l.temperatura_c, None);
        assert_eq!(l.consumo_w, None);
        assert_eq!(l.persistencia, Some(false));
        assert_eq!(l.pstate, None);
        assert_eq!(resumo_da_gpu(&l), "");
        assert_eq!(ler_nvidia_smi(""), None);
        assert_eq!(ler_nvidia_smi("No devices were found"), None);
    }
}
