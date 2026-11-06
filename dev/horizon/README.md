# Ambiente Horizon isolado

Launcher, jogo e compilador de mods nativos em um container descartável, dentro de um gamescope, no padrão da skill
`jogo-isolado`. Mods nativos não têm sandbox, então o container é a proteção: raiz somente leitura, sem capabilities,
Wayland em sandbox, áudio que só reproduz e rede só para a internet.

```sh
./instalar                        # copia para ~/.local/opt/horizon e constrói a imagem
horizon conta sign-in             # login, uma vez
horizon                           # abre online
horizon offline                   # abre sem conta
JOGO_CMD='horizon-launcher native-build ~/Games/Horizon/mods/<pasta> --profile dev --trust-native-code' horizon
```

O `--trust-native-code` é a decisão de quem opera: só use em código que você leu.

O atalho `Horizon` do menu abre o mesmo script.

Antes de instalar um mod suspeito, tire um snapshot da pasta de dados com o `snapshot` da skill `jogo-isolado`. O
container nunca enxerga os snapshots.

| Caminho | Conteúdo |
|---|---|
| `~/.local/share/jogo-isolado/horizon` | Launcher, jogo e mundos (escrita) |
| `~/.local/share/jogo-isolado/horizon-conta` | Conta Microsoft, nunca montada no container do jogo |
| `~/Games/Horizon/mods` | Mods (somente leitura) |
| `~/.local/state/jogo-isolado/horizon` | Logs |

Para operar o jogo de fora, use o `horizon-ctl` da versão em uso, que fala pelo socket de `HORIZON_CONTROL`:
`state`, `screenshot`, `command`, `click-button` e outros. Com o monitor desligado, o jogo continuou renderizando.
Para testar sem janela, passe o gamescope em `JOGO_CMD`: `gamescope -W 1280 -H 720 -w 1280 -h 720 -r 15 --backend headless -- env -u
WAYLAND_DISPLAY jogar --world <mundo> --creative --width 1280 --height 720 --control-socket $HORIZON_CONTROL`. O `-r 15` poupa a GPU para
quem estiver jogando outra coisa. Ao compilar no desktop, use `docker update --cpus 6 horizon` e `CARGO_BUILD_JOBS=6`.

O refresh token fica fora do alcance dos mods. No modo online, o access token entra no processo do jogo e vale cerca de
24 h: um mod malicioso instalado consegue lê-lo a cada abertura online. `horizon offline` não entrega nenhum token.
