# meowsystem-performance

O applet do **modo performance** na topbar do COSMIC: mostra e troca o perfil de energia do
`system76-power`, mostra o modo gráfico e o estado da NVIDIA, e avisa quando trocar o modo
gráfico ainda pede um reinício.

Faz parte do [MeowSystem](https://github.com/vitoriamariadb/MeowSystem) e usa a mesma base do
[meowsystem-leitura](https://github.com/vitoriamariadb/meowsystem-leitura): o mesmo libcosmic
pinado, a mesma estrutura e o mesmo tema.

## O que ele faz

- Na barra: um ponteiro que aponta o perfil (à direita **Desempenho**, ao centro
  **Equilibrado**, à esquerda **Economia**). Passar o mouse mostra o nome.
- No popup:
  - os três perfis, o marcado na cor do acento; um clique troca na hora;
  - **Modo gráfico**: Integrada, Híbrida ou NVIDIA. Trocar pede confirmação, porque só vale
    depois de reiniciar; até o reinício o popup mostra «Reinicie para usar o modo …»;
  - **NVIDIA**: numa linha, o nome curto (sem «NVIDIA», «GeForce», «Laptop GPU») e, à direita,
    temperatura, consumo, uso e estado de energia. Com a placa em suspensão o applet mostra «Dormindo» e não chama o
    `nvidia-smi`, que a acordaria.
- Num computador sem troca de placa (`GetSwitchable` falso), o modo gráfico não aparece.

```bash
meowsystem-performance --estado   # imprime o que o applet leria, sem trocar nada
```

## Como ele fala com o sistema

| o quê | por onde |
|---|---|
| perfil de energia | D-Bus do sistema, `com.system76.PowerDaemon`: `GetProfile`, `Performance`, `Balanced`, `Battery` |
| modo gráfico | o mesmo D-Bus: `GetSwitchable`, `GetGraphics`, `SetGraphics` |
| perfil da placa | `/sys/class/platform-profile/*/profile` (só leitura) |
| NVIDIA | `/sys/bus/pci/devices/*/power/runtime_status` e `nvidia-smi --query-gpu=...` (só leitura) |
| reinício pendente | o modo do começo do boot, guardado em `~/.local/state/cosmic/io.github.meowsystem.Performance/v1/` |

Sem `sudo`: a troca de perfil e de modo gráfico passa pelo polkit do `system76-power`. No
MeowSystem a regra `50-system76-power.rules` libera o grupo `sudo` numa sessão local ativa.

O `system76-power` responde o modo gráfico **configurado**, que só vale no próximo boot. O
applet guarda o modo do começo de cada boot (pelo `boot_id` do kernel) e compara: diferente é
reinício pendente. Configurar a integrada com o módulo `nvidia` ainda carregado também é.

## Compilar e instalar

Precisa de Rust 1.93 ou mais novo e das bibliotecas de desenvolvimento do libcosmic
(`libxkbcommon-dev`, `libwayland-dev`, `pkg-config`).

```bash
cargo build --release --locked
cargo test --locked          # o D-Bus de verdade nunca é chamado: os testes usam um falso
./install.sh                 # binário em ~/.local/bin, .desktop em ~/.local/share/applications
./install.sh --remover       # desfaz
```

Depois, acrescente **Modo performance** em Configurações > Área de trabalho > Painel > Applets.

## Tema

Estilo de applet do libcosmic e desenhos `symbolic` (`data/icones/`), repintados com a cor do
tema. Quando o theme pack traz `~/.config/meowsystem/extensoes/io.github.meowsystem.Performance/tema.conf`
(ou `$MEOWSYSTEM_TEMA`), as cores dele valem por cima do COSMIC: `acento` no perfil marcado e no
modo gráfico marcado, `destaque` no texto sobre o acento, `fundo`, `borda`, `texto` e `bloco` no
popup. Linhas `chave = #rrggbb`; sem o arquivo, vale o tema do COSMIC.

## Licença

GPL-3.0, a mesma do MeowSystem. Ver [`LICENSE`](LICENSE).
