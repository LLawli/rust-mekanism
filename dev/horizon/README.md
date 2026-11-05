# Ambiente Horizon isolado

Launcher, jogo e compilador de mods nativos em um container descartável, dentro de um gamescope, no padrão da skill
`jogo-isolado`. Mods nativos não têm sandbox, então o container é a proteção: raiz somente leitura, sem capabilities,
Wayland em sandbox, áudio que só reproduz e rede só para a internet.

```sh
./instalar                        # copia para ~/.local/opt/horizon e constrói a imagem
horizon                           # abre o jogo
JOGO_CMD='horizon-launcher account sign-in' horizon
JOGO_CMD='horizon-launcher native-build ~/Games/Horizon/mods/<pasta> --profile dev --trust-native-code' horizon
```

O `--trust-native-code` é a decisão de quem opera: só use em código que você leu.

O atalho `Horizon` do menu abre o mesmo script.

Antes de instalar um mod suspeito, tire um snapshot da pasta de dados com o `snapshot` da skill `jogo-isolado`. O
container nunca enxerga os snapshots.

| Caminho | Conteúdo |
|---|---|
| `~/.local/share/jogo-isolado/horizon` | Launcher, jogo, mundos e sessão da conta (escrita) |
| `~/Games/Horizon/mods` | Mods (somente leitura) |
| `~/.local/state/jogo-isolado/horizon` | Logs |

Para operar o jogo de fora, use o `horizon-ctl` da versão em uso, que fala pelo socket de `HORIZON_CONTROL`:
`state`, `screenshot`, `command`, `click-button` e outros. Com o monitor desligado, o jogo continuou renderizando.
`HORIZON_BACKEND=headless horizon` troca o backend do gamescope (não testado).

Um mod malicioso alcança a pasta de dados, inclusive a sessão da conta Microsoft que fica lá.
