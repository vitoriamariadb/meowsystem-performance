// A conversa com o system76-power, atrás de um trait: o applet de verdade fala
// D-Bus no barramento do sistema, e os testes falam com um falso que só anota.
// Trocar perfil ou modo gráfico de verdade num teste escreveria no sistema.

use std::future::Future;

use crate::modelo::{ModoGrafico, Perfil};

pub type Resultado<T> = Result<T, String>;

pub trait Energia: Clone + Send + Sync + 'static {
    fn perfil(&self) -> impl Future<Output = Resultado<Perfil>> + Send;
    fn trocar_perfil(&self, p: Perfil) -> impl Future<Output = Resultado<()>> + Send;
    fn grafico(&self) -> impl Future<Output = Resultado<ModoGrafico>> + Send;
    fn trocar_grafico(&self, m: ModoGrafico) -> impl Future<Output = Resultado<()>> + Send;
    /// `false` num desktop sem troca de placa: o popup esconde o modo gráfico.
    fn alternavel(&self) -> impl Future<Output = Resultado<bool>> + Send;
}

/// O que o applet lê de uma vez a cada passo do relógio.
#[derive(Clone, Debug, PartialEq)]
pub struct Leitura {
    pub perfil: Resultado<Perfil>,
    pub grafico: Option<ModoGrafico>,
}

pub async fn ler<E: Energia>(e: &E) -> Leitura {
    let perfil = e.perfil().await;
    let grafico = match e.alternavel().await {
        Ok(true) => e.grafico().await.ok(),
        _ => None,
    };
    Leitura { perfil, grafico }
}

/// Troca e relê: a resposta é o perfil que o daemon diz ter depois da troca,
/// não o que foi pedido.
pub async fn aplicar_perfil<E: Energia>(e: &E, p: Perfil) -> Resultado<Perfil> {
    e.trocar_perfil(p).await?;
    e.perfil().await
}

/// Troca e relê o modo gráfico. Não troca para o modo em que já está, e não
/// oferece o de computação (ver `ModoGrafico::OFERECIDOS`).
pub async fn aplicar_grafico<E: Energia>(e: &E, m: ModoGrafico) -> Resultado<ModoGrafico> {
    if !ModoGrafico::OFERECIDOS.contains(&m) {
        return Err(format!("o modo {} não é oferecido", m.nome()));
    }
    if e.grafico().await? != m {
        e.trocar_grafico(m).await?;
    }
    e.grafico().await
}

/// O system76-power de verdade, pelo D-Bus do sistema. O polkit
/// (`com.system76.powerdaemon.*`) libera quem está no grupo `sudo` sem senha.
/// A conexão ao barramento é aberta na primeira chamada e reaproveitada: o
/// applet lê a cada 5 s.
#[derive(Clone, Debug, Default)]
pub struct System76 {
    conexao: std::sync::Arc<tokio::sync::OnceCell<zbus::Connection>>,
}

const DESTINO: &str = "com.system76.PowerDaemon";
const CAMINHO: &str = "/com/system76/PowerDaemon";
const INTERFACE: &str = "com.system76.PowerDaemon";

impl System76 {
    async fn proxy(&self) -> Resultado<zbus::Proxy<'static>> {
        let conexao = self
            .conexao
            .get_or_try_init(zbus::Connection::system)
            .await
            .map_err(erro)?;
        zbus::Proxy::new(conexao, DESTINO, CAMINHO, INTERFACE).await.map_err(erro)
    }

    async fn texto(&self, metodo: &str) -> Resultado<String> {
        let p = self.proxy().await?;
        p.call::<_, _, String>(metodo, &()).await.map_err(erro)
    }
}

fn erro(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(nome, msg, _) => match msg {
            Some(m) if !m.is_empty() => m,
            _ => nome.to_string(),
        },
        outro => outro.to_string(),
    }
}

impl Energia for System76 {
    async fn perfil(&self) -> Resultado<Perfil> {
        let v = self.texto("GetProfile").await?;
        Perfil::do_daemon(&v).ok_or_else(|| format!("perfil desconhecido: {v}"))
    }

    async fn trocar_perfil(&self, p: Perfil) -> Resultado<()> {
        let proxy = self.proxy().await?;
        proxy.call::<_, _, ()>(p.metodo(), &()).await.map_err(erro)
    }

    async fn grafico(&self) -> Resultado<ModoGrafico> {
        let v = self.texto("GetGraphics").await?;
        ModoGrafico::do_daemon(&v).ok_or_else(|| format!("modo gráfico desconhecido: {v}"))
    }

    async fn trocar_grafico(&self, m: ModoGrafico) -> Resultado<()> {
        let proxy = self.proxy().await?;
        proxy
            .call::<_, _, ()>("SetGraphics", &(m.para_daemon(),))
            .await
            .map_err(erro)
    }

    async fn alternavel(&self) -> Resultado<bool> {
        let p = self.proxy().await?;
        p.call::<_, _, bool>("GetSwitchable", &()).await.map_err(erro)
    }
}

#[cfg(test)]
pub mod falso {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Um system76-power de mentira: guarda o estado, anota cada troca e pode
    /// recusar como o polkit recusaria.
    #[derive(Clone)]
    pub struct Falso {
        pub estado: Arc<Mutex<(Perfil, ModoGrafico)>>,
        pub trocas: Arc<Mutex<Vec<String>>>,
        pub alternavel: bool,
        pub recusa: Option<String>,
    }

    impl Falso {
        pub fn novo(p: Perfil, m: ModoGrafico) -> Self {
            Self {
                estado: Arc::new(Mutex::new((p, m))),
                trocas: Arc::default(),
                alternavel: true,
                recusa: None,
            }
        }

        pub fn trocas(&self) -> Vec<String> {
            self.trocas.lock().unwrap().clone()
        }
    }

    impl Energia for Falso {
        async fn perfil(&self) -> Resultado<Perfil> {
            Ok(self.estado.lock().unwrap().0)
        }

        async fn trocar_perfil(&self, p: Perfil) -> Resultado<()> {
            if let Some(r) = &self.recusa {
                return Err(r.clone());
            }
            self.trocas.lock().unwrap().push(p.metodo().to_string());
            self.estado.lock().unwrap().0 = p;
            Ok(())
        }

        async fn grafico(&self) -> Resultado<ModoGrafico> {
            Ok(self.estado.lock().unwrap().1)
        }

        async fn trocar_grafico(&self, m: ModoGrafico) -> Resultado<()> {
            if let Some(r) = &self.recusa {
                return Err(r.clone());
            }
            self.trocas.lock().unwrap().push(format!("SetGraphics({})", m.para_daemon()));
            self.estado.lock().unwrap().1 = m;
            Ok(())
        }

        async fn alternavel(&self) -> Resultado<bool> {
            Ok(self.alternavel)
        }
    }

    /// Os futuros do falso ficam prontos no primeiro `poll`.
    pub fn ja<F: Future>(f: F) -> F::Output {
        let mut f = std::pin::pin!(f);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match f.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(v) => v,
            std::task::Poll::Pending => panic!("o falso não deveria esperar"),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::falso::{Falso, ja};
    use super::*;

    #[test]
    fn trocar_perfil_chama_o_metodo_do_perfil_e_rele() {
        let f = Falso::novo(Perfil::Desempenho, ModoGrafico::Nvidia);
        assert_eq!(ja(aplicar_perfil(&f, Perfil::Economia)), Ok(Perfil::Economia));
        assert_eq!(f.trocas(), ["Battery"]);
    }

    #[test]
    fn recusa_do_polkit_chega_como_erro_e_nada_muda() {
        let mut f = Falso::novo(Perfil::Equilibrado, ModoGrafico::Hibrida);
        f.recusa = Some("Operation not permitted".into());
        assert_eq!(ja(aplicar_perfil(&f, Perfil::Desempenho)), Err("Operation not permitted".into()));
        assert_eq!(ja(aplicar_grafico(&f, ModoGrafico::Nvidia)), Err("Operation not permitted".into()));
        assert!(f.trocas().is_empty());
        assert_eq!(ja(ler(&f)).perfil, Ok(Perfil::Equilibrado));
    }

    #[test]
    fn trocar_grafico_so_quando_muda_e_nunca_para_computacao() {
        let f = Falso::novo(Perfil::Equilibrado, ModoGrafico::Hibrida);
        assert_eq!(ja(aplicar_grafico(&f, ModoGrafico::Hibrida)), Ok(ModoGrafico::Hibrida));
        assert!(f.trocas().is_empty());
        assert!(ja(aplicar_grafico(&f, ModoGrafico::Computacao)).is_err());
        assert_eq!(ja(aplicar_grafico(&f, ModoGrafico::Integrada)), Ok(ModoGrafico::Integrada));
        assert_eq!(f.trocas(), ["SetGraphics(integrated)"]);
    }

    #[test]
    fn desktop_sem_troca_de_placa_nao_tem_modo_grafico() {
        let mut f = Falso::novo(Perfil::Desempenho, ModoGrafico::Nvidia);
        f.alternavel = false;
        assert_eq!(ja(ler(&f)), Leitura { perfil: Ok(Perfil::Desempenho), grafico: None });
    }
}
