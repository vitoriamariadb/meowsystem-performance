// meowsystem-performance — o modo performance na topbar do COSMIC.
//
// Na barra, o ponteiro do perfil de energia do system76-power. No popup, os
// três perfis, o perfil que a placa-mãe seguiu, o modo gráfico e a NVIDIA.
// Trocar perfil é na hora; trocar modo gráfico pede confirmação e só vale
// depois de reiniciar, e o popup diz isso até o reinício acontecer.

mod energia;
mod modelo;
mod sistema;

use std::path::Path;
use std::time::Duration;

use cosmic::app::Core;
use cosmic::applet::{menu_button, padded_control};
use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use cosmic::iced::{Alignment, Length, Limits, Subscription, window, window::Id};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{Column, Row, button, divider, icon, text};
use cosmic::{Action, Element, Task};

use energia::{Leitura, System76};
use modelo::{EstadoGpu, ModoGrafico, Perfil};

/// O `app_id`: o basename do `.desktop` que o cosmic-panel procura.
const ID: &str = "io.github.meowsystem.Performance";
/// O estado do applet em `~/.local/state/cosmic/<ID>/v1/`: só a fotografia do
/// modo gráfico em que este boot começou.
const NS_ESTADO_V: u64 = 1;

const ARTE_DESEMPENHO: &[u8] = include_bytes!("../data/icones/performance-desempenho.svg");
const ARTE_EQUILIBRADO: &[u8] = include_bytes!("../data/icones/performance-equilibrado.svg");
const ARTE_ECONOMIA: &[u8] = include_bytes!("../data/icones/performance-economia.svg");
const ARTE_GPU: &[u8] = include_bytes!("../data/icones/performance-gpu.svg");
const ARTE_GRAFICO: &[u8] = include_bytes!("../data/icones/performance-grafico.svg");
const ARTE_REINICIAR: &[u8] = include_bytes!("../data/icones/performance-reiniciar.svg");

const ICONE_ROTULO: u16 = 16;
/// O perfil pode mudar por fora (o `nitro-profiled` troca na tomada): a barra
/// confere a cada 5 s. A NVIDIA só é lida com o popup aberto.
const PASSO_LEITURA: Duration = Duration::from_secs(5);
const PASSO_GPU: Duration = Duration::from_secs(3);

const SYS: &str = "/sys";

fn arte_do_perfil(p: Option<Perfil>) -> &'static [u8] {
    match p {
        Some(Perfil::Desempenho) => ARTE_DESEMPENHO,
        Some(Perfil::Economia) => ARTE_ECONOMIA,
        _ => ARTE_EQUILIBRADO,
    }
}

fn selo(arte: &'static [u8]) -> Element<'static, Message> {
    icon::icon(icon::from_svg_bytes(arte).symbolic(true))
        .size(ICONE_ROTULO)
        .into()
}

struct Applet {
    core: Core,
    popup: Option<Id>,
    energia: System76,
    perfil: Option<Perfil>,
    /// O último erro do daemon (polkit, daemon fora do ar), como dica curta.
    erro: Option<String>,
    trocando_perfil: bool,
    /// `None` num desktop sem troca de placa.
    grafico: Option<ModoGrafico>,
    grafico_no_boot: Option<ModoGrafico>,
    modulo_nvidia: bool,
    /// O modo que ela clicou e ainda não confirmou.
    confirmar: Option<ModoGrafico>,
    trocando_grafico: Option<ModoGrafico>,
    placa: Option<String>,
    gpu: Option<EstadoGpu>,
    sondando: bool,
    estado: Option<Config>,
}

#[derive(Clone, Debug)]
enum Message {
    AbreFecha,
    PopupFechou(Id),
    Tique,
    Leu(Leitura),
    Escolher(Perfil),
    PerfilTrocado(Result<Perfil, String>),
    PedirGrafico(ModoGrafico),
    Confirmar,
    Cancelar,
    GraficoTrocado(Result<ModoGrafico, String>),
    TiqueGpu,
    Gpu(EstadoGpu),
    /// A dica da barra abre e fecha por superfície própria.
    Surface(cosmic::surface::Action),
}

impl Applet {
    fn ler(&self) -> Task<Action<Message>> {
        let e = self.energia.clone();
        cosmic::task::future(async move { Message::Leu(energia::ler(&e).await) })
    }

    fn sondar(&mut self) -> Task<Action<Message>> {
        if self.sondando {
            return Task::none();
        }
        self.sondando = true;
        cosmic::task::future(async move {
            let estado = tokio::task::spawn_blocking(|| sistema::sondar_gpu(Path::new(SYS), sistema::nvidia_smi))
                .await
                .unwrap_or(EstadoGpu::SemResposta);
            Message::Gpu(estado)
        })
    }

    /// A fotografia do boot, lida e gravada no estado uma vez por boot.
    fn fotografar(&mut self, configurado: ModoGrafico) {
        if self.grafico_no_boot.is_some() {
            return;
        }
        let Some(boot) = sistema::id_do_boot() else {
            self.grafico_no_boot = Some(configurado);
            return;
        };
        let gravada = self.estado.as_ref().and_then(|e| {
            let b = e.get::<String>("boot").ok()?;
            let m = ModoGrafico::do_daemon(&e.get::<String>("modo").ok()?)?;
            Some((b, m))
        });
        let (modo, gravar) =
            modelo::fotografia_do_boot(gravada.as_ref().map(|(b, m)| (b.as_str(), *m)), &boot, configurado);
        if gravar && let Some(e) = self.estado.as_ref() {
            let _ = e.set::<String>("boot", boot);
            let _ = e.set::<String>("modo", modo.para_daemon().to_string());
        }
        self.grafico_no_boot = Some(modo);
    }

    fn reinicio_pendente(&self) -> Option<ModoGrafico> {
        let (cfg, boot) = (self.grafico?, self.grafico_no_boot?);
        modelo::reinicio_pendente(cfg, boot, self.modulo_nvidia).then_some(cfg)
    }

    fn linha(arte: &'static [u8], rotulo: String, valor: Option<String>) -> Element<'static, Message> {
        let mut r = Row::new()
            .push(selo(arte))
            .push(text::body(rotulo).width(Length::Fill))
            .spacing(8)
            .align_y(Alignment::Center);
        if let Some(v) = valor {
            r = r.push(text::body(v));
        }
        r.into()
    }

    fn bloco_perfis(&self) -> Element<'_, Message> {
        let mut col = Column::new();
        for p in Perfil::TODOS {
            let marcado = self.perfil == Some(p);
            let mut linha = Row::new()
                .push(selo(arte_do_perfil(Some(p))))
                .push(text::body(p.nome()).width(Length::Fill))
                .spacing(8)
                .align_y(Alignment::Center);
            if marcado {
                linha = linha.push(icon::from_name("object-select-symbolic").size(ICONE_ROTULO));
            }
            let pode = !self.trocando_perfil && !marcado;
            col = col.push(menu_button(linha).on_press_maybe(pode.then_some(Message::Escolher(p))));
        }
        if let Some(placa) = &self.placa {
            col = col.push(padded_control(text::caption(format!(
                "Placa: {}",
                modelo::nome_do_perfil_da_placa(placa)
            ))));
        }
        col.into()
    }

    fn bloco_grafico(&self) -> Option<Element<'_, Message>> {
        let atual = self.grafico?;
        let mut col = Column::new().push(padded_control(Self::linha(ARTE_GRAFICO, "Modo gráfico".into(), None)));
        let mut opcoes = Row::new().spacing(8);
        for m in ModoGrafico::OFERECIDOS {
            let livre = self.trocando_grafico.is_none() && m != atual;
            let b = if m == atual {
                button::suggested(m.nome())
            } else {
                button::standard(m.nome())
            };
            opcoes = opcoes.push(b.on_press_maybe(livre.then_some(Message::PedirGrafico(m))));
        }
        if atual == ModoGrafico::Computacao {
            opcoes = opcoes.push(text::body(atual.nome()));
        }
        col = col.push(padded_control(opcoes));

        if let Some(m) = self.trocando_grafico {
            col = col.push(padded_control(text::caption(format!("Trocando para {}…", m.nome()))));
        } else if let Some(m) = self.confirmar {
            col = col.push(padded_control(
                Column::new()
                    .push(text::caption(format!("{} só vale depois de reiniciar.", m.nome())))
                    .push(
                        Row::new()
                            .push(button::suggested("Trocar").on_press(Message::Confirmar))
                            .push(button::standard("Cancelar").on_press(Message::Cancelar))
                            .spacing(8),
                    )
                    .spacing(8),
            ));
        } else if let Some(m) = self.reinicio_pendente() {
            col = col.push(padded_control(
                Row::new()
                    .push(selo(ARTE_REINICIAR))
                    .push(text::caption(format!("Reinicie para usar o modo {}.", m.nome())).width(Length::Fill))
                    .spacing(8)
                    .align_y(Alignment::Center),
            ));
        }
        Some(col.into())
    }

    fn bloco_gpu(&self) -> Option<Element<'_, Message>> {
        let texto = match self.gpu.as_ref()? {
            EstadoGpu::Desligada => return None,
            EstadoGpu::Dormindo => ("NVIDIA".to_string(), Some("Dormindo".to_string()), None),
            EstadoGpu::SemResposta => ("NVIDIA".to_string(), Some("Sem resposta".to_string()), None),
            EstadoGpu::Ativa(l) => {
                let nome = l.nome.trim_start_matches("NVIDIA ").trim_start_matches("GeForce ").to_string();
                let persist = l.persistencia.map(|p| {
                    if p { "Persistência ligada" } else { "Persistência desligada" }.to_string()
                });
                (nome, Some(modelo::resumo_da_gpu(l)).filter(|s| !s.is_empty()), persist)
            }
        };
        let (nome, valor, persist) = texto;
        let mut col = Column::new().push(Self::linha(ARTE_GPU, nome, valor));
        if let Some(p) = persist {
            col = col.push(text::caption(p));
        }
        Some(padded_control(col.spacing(4)).into())
    }
}

impl cosmic::Application for Applet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let applet = Self {
            core,
            popup: None,
            energia: System76::default(),
            perfil: None,
            erro: None,
            trocando_perfil: false,
            grafico: None,
            grafico_no_boot: None,
            modulo_nvidia: sistema::modulo_nvidia_carregado(Path::new(SYS)),
            confirmar: None,
            trocando_grafico: None,
            placa: sistema::perfil_da_placa(Path::new(SYS)),
            gpu: None,
            sondando: false,
            estado: Config::new_state(ID, NS_ESTADO_V).ok(),
        };
        let t = applet.ler();
        (applet, t)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupFechou(id))
    }

    fn update(&mut self, message: Message) -> Task<Action<Self::Message>> {
        match message {
            Message::AbreFecha => return self.abre_fecha(),
            Message::PopupFechou(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.confirmar = None;
                }
            }
            Message::Tique => {
                self.placa = sistema::perfil_da_placa(Path::new(SYS));
                self.modulo_nvidia = sistema::modulo_nvidia_carregado(Path::new(SYS));
                return self.ler();
            }
            Message::Leu(l) => {
                match l.perfil {
                    Ok(p) if !self.trocando_perfil => {
                        self.perfil = Some(p);
                        self.erro = None;
                    }
                    Ok(_) => {}
                    Err(e) => self.erro = Some(e),
                }
                if self.trocando_grafico.is_none() {
                    self.grafico = l.grafico;
                }
                if let Some(m) = self.grafico {
                    self.fotografar(m);
                }
            }
            Message::Escolher(p) => {
                self.trocando_perfil = true;
                let e = self.energia.clone();
                return cosmic::task::future(async move {
                    Message::PerfilTrocado(energia::aplicar_perfil(&e, p).await)
                });
            }
            Message::PerfilTrocado(r) => {
                self.trocando_perfil = false;
                match r {
                    Ok(p) => {
                        self.perfil = Some(p);
                        self.erro = None;
                    }
                    Err(e) => self.erro = Some(e),
                }
                self.placa = sistema::perfil_da_placa(Path::new(SYS));
            }
            Message::PedirGrafico(m) => self.confirmar = Some(m),
            Message::Cancelar => self.confirmar = None,
            Message::Confirmar => {
                if let Some(m) = self.confirmar.take() {
                    self.trocando_grafico = Some(m);
                    let e = self.energia.clone();
                    return cosmic::task::future(async move {
                        Message::GraficoTrocado(energia::aplicar_grafico(&e, m).await)
                    });
                }
            }
            Message::GraficoTrocado(r) => {
                self.trocando_grafico = None;
                match r {
                    Ok(m) => {
                        self.grafico = Some(m);
                        self.erro = None;
                    }
                    Err(e) => self.erro = Some(e),
                }
            }
            Message::TiqueGpu => return self.sondar(),
            Message::Surface(a) => return tarefa_de_superficie(a),
            Message::Gpu(g) => {
                self.sondando = false;
                self.gpu = Some(g);
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut fios = vec![cosmic::iced::time::every(PASSO_LEITURA).map(|_| Message::Tique)];
        if self.popup.is_some() {
            fios.push(cosmic::iced::time::every(PASSO_GPU).map(|_| Message::TiqueGpu));
        }
        Subscription::batch(fios)
    }

    fn view(&self) -> Element<'_, Message> {
        let botao = self
            .core
            .applet
            .icon_button_from_handle(icon::from_svg_bytes(arte_do_perfil(self.perfil)).symbolic(true))
            .on_press_down(Message::AbreFecha);
        let dica = match self.perfil {
            Some(p) => p.nome(),
            None => "Modo performance",
        };
        self.core
            .applet
            .applet_tooltip(botao, dica, self.popup.is_some(), Message::Surface, None)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let mut conteudo: Column<'_, Message, cosmic::Theme> = Column::new().push(self.bloco_perfis());
        if let Some(e) = &self.erro {
            conteudo = conteudo.push(padded_control(text::caption(e.clone())));
        }
        if let Some(g) = self.bloco_grafico() {
            conteudo = conteudo.push(padded_control(divider::horizontal::default())).push(g);
        }
        if let Some(g) = self.bloco_gpu() {
            conteudo = conteudo.push(padded_control(divider::horizontal::default())).push(g);
        }
        self.core
            .applet
            .popup_container(conteudo.padding([8, 0]))
            .into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl Applet {
    fn abre_fecha(&mut self) -> Task<Action<Message>> {
        if let Some(popup) = self.popup.take() {
            self.confirmar = None;
            return tarefa_de_superficie(destroy_popup(popup));
        }
        let Some(pai) = self.core.main_window_id() else {
            eprintln!("meowsystem-performance: sem janela principal — popup não abre");
            return Task::none();
        };
        let abrir = tarefa_de_superficie(app_popup::<Applet>(
            |_| Default::default(),
            move |estado: &mut Applet| {
                let popup = Id::unique();
                let mut ajustes = estado.core.applet.get_popup_settings(pai, popup, None, None, None);
                ajustes.positioner.size_limits = Limits::NONE
                    .max_width(400.0)
                    .min_width(300.0)
                    .min_height(120.0)
                    .max_height(1080.0);
                estado.popup = Some(popup);
                ajustes
            },
            None,
        ));
        Task::batch([abrir, self.ler(), self.sondar()])
    }
}

fn tarefa_de_superficie(acao: cosmic::surface::Action) -> Task<Action<Message>> {
    cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(acao)))
}

/// `--estado` imprime o que o applet leria e sai. Só lê: não troca nada.
fn imprimir_estado() {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return eprintln!("meowsystem-performance: {e}"),
    };
    let l = rt.block_on(energia::ler(&System76::default()));
    match l.perfil {
        Ok(p) => println!("perfil: {}", p.nome()),
        Err(e) => println!("perfil: erro: {e}"),
    }
    let sys = Path::new(SYS);
    println!("placa: {}", sistema::perfil_da_placa(sys).as_deref().map_or("-".into(), modelo::nome_do_perfil_da_placa));
    println!("modo gráfico: {}", l.grafico.map_or("-", ModoGrafico::nome));
    println!("módulo nvidia: {}", if sistema::modulo_nvidia_carregado(sys) { "carregado" } else { "fora" });
    let gpu = match sistema::sondar_gpu(sys, sistema::nvidia_smi) {
        EstadoGpu::Desligada => "desligada".to_string(),
        EstadoGpu::Dormindo => "dormindo".to_string(),
        EstadoGpu::SemResposta => "sem resposta".to_string(),
        EstadoGpu::Ativa(g) => format!("{} · {} · persistência {:?}", g.nome, modelo::resumo_da_gpu(&g), g.persistencia),
    };
    println!("gpu: {gpu}");
}

fn main() -> cosmic::iced::Result {
    if std::env::args().any(|a| a == "--estado") {
        imprimir_estado();
        return Ok(());
    }
    cosmic::applet::run::<Applet>(())
}
