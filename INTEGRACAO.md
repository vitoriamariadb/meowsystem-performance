# Integração com o MeowSystem

O modo performance vive hoje em scripts do zsh (`~/.config/zsh/scripts/nitro/`), instalados
pelo `install.sh` do zsh (`_step_nitro`) e mantidos pelo `ritual-aurora-self-heal.sh`. Este
applet é a cara dele na topbar. Nada do MeowSystem nem do zsh foi mudado: o que está abaixo é
a sprint que liga as pontas.

## Onde está

| | |
|---|---|
| repositório | <https://github.com/vitoriamariadb/meowsystem-performance> |
| clone (SSH desta máquina) | `git@github.com-personal:vitoriamariadb/meowsystem-performance.git` |
| app id | `io.github.meowsystem.Performance` |
| binário | `meowsystem-performance` (`--estado` imprime a leitura e sai) |
| `.desktop` | `data/io.github.meowsystem.Performance.desktop.in` (o `@bindir@` vira o diretório do binário) |
| libcosmic | `1f6dc991eaa5115a09be2505cc8a323e5b2b0bff`, o mesmo do meowsystem-leitura |

## Build

```bash
cargo build --release --locked      # target/release/meowsystem-performance
cargo test --locked
```

O `Cargo.lock` é o do meowsystem-leitura mais este pacote: `zbus` e `tokio` já estavam na
árvore do libcosmic, e nenhuma crate nova entrou.

## O que o applet lê e escreve

| | lê | escreve |
|---|---|---|
| `com.system76.PowerDaemon` (D-Bus do sistema) | `GetProfile`, `GetSwitchable`, `GetGraphics` | `Performance`/`Balanced`/`Battery` e `SetGraphics`, só por clique |
| `/sys/class/platform-profile/*/{name,profile}` | o perfil da placa | nada |
| `/sys/bus/pci/devices/*/{vendor,class,power/runtime_status}`, `/sys/module/nvidia` | a NVIDIA e se ela dorme | nada |
| `nvidia-smi --query-gpu=name,temperature.gpu,power.draw,utilization.gpu,persistence_mode,pstate` | com a placa acordada e o popup aberto | nada |
| `~/.local/state/cosmic/io.github.meowsystem.Performance/v1/{boot,modo}` | o modo do começo do boot | uma vez por boot |

Nenhuma chave de configuração: o estado é do `system76-power`. O applet não mexe na
persistência da NVIDIA (é o drop-in `10-nitro-persistence.conf` do zsh); só a mostra.

## Quem continua fazendo o quê

- O `nitro-profiled` continua dono da placa: ele ouve o `PowerProfileSwitch` e põe o
  `platform_profile` no par do perfil (Desempenho → `balanced-performance`, Equilibrado →
  `balanced`, Economia → `low-power`), destrava a frequência que o Economia prende e troca o
  perfil na tomada. O applet só troca o perfil do `system76-power`; o resto segue sozinho, e a
  linha «Placa» mostra o resultado.
- A regra `50-system76-power.rules` (polkit, instalada pelo self-heal) é o que deixa o clique
  trocar sem senha.

## O que o MeowSystem muda para consumir este repositório (a sprint)

1. **Um passo do instalador**, irmão do «Applet do modo de leitura (compilação e sombra)»: um
   `scripts/performance_build.sh` com o mesmo contrato do `leitura_build.sh` (`--conferir`,
   `--aplicar`, `--reverter`, carimbo, binário antes da sombra, fail-safe da sombra órfã), mas
   clonando este repositório num commit pinado em `~/.local/state/meowsystem/performance/src`,
   como o `midia_build.sh` faz com o `PINO`. O `applet_pino.sh` ganha o nome `performance`.
2. **Uma chave no `meow.conf.exemplo`**, `PERFORMANCE_APPLET="sim"`, com o mesmo desligar que
   o `LEITURA_APPLET`: «nao» remove binário e sombra.
3. **O doctor**: a linha do applet, com conserto (o `fix_` que chama o build), sem repetir a
   dívida do `leiturabin` no `SEM_CONSERTO`.
4. **A arte no pack.** Os seis desenhos de `data/icones/performance-*.svg` entram em
   `packs/meowsystem/icones/sistema/`, e um teste confere que as cópias daqui batem.
5. **A fronteira.** `docs/FRONTEIRA.md` ganha as linhas do binário, da sombra
   `~/.local/share/applications/io.github.meowsystem.Performance.desktop` e do estado em
   `~/.local/state/cosmic/io.github.meowsystem.Performance/`. A lista de applets da topbar
   continua de quem usa: o MeowSystem só avisa quando o applet não está nela.
6. **Fora de máquina com `system76-power`** o passo pula, como o do leitura pula sem COSMIC.

## Tema

O mesmo contrato do meowsystem-leitura: `cosmic::applet::style()`, o `popup_container` do
libcosmic e desenhos `symbolic` repintados com a cor do tema do COSMIC. O MeowSystem não
escreve nada para este applet.
