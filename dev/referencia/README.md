# Mekanism de referência

O mod original, compilado do commit que o port segue, para comparar comportamento lado a lado. Roda no mesmo isolamento do
Horizon: é código de terceiros, e o `runClient` não usa conta.

```sh
./instalar <repositório do Mekanism>   # precisa do jogo-horizon já construído
mekanism-ref offline                   # compila na primeira vez e abre o jogo
JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon compileJava' mekanism-ref
JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon runGameTestServer' mekanism-ref
```

O `runGameTestServer` roda sem janela e sem ninguém: os testes do Mekanism e os de `testes/`, que gravam o estado a cada tick,
depois de `TransmitterNetworkRegistry.onTick`, em `src/runs/gameTestServer/trace/<cenário>.jsonl`. Os testes em Rust comparam
com esses rastros. Os das máquinas ficam copiados em `rastros/`, e `mods/mekanism/tests/chamber_trace.rs` os repete tick a tick.

Para comparar a imagem, `testes/CenaCabo.java` monta a cena do cabo no cliente e tira os prints pelo próprio jogo. Precisa de um
mundo em `src/runs/client/saves/GameTestWorld`, do arquivo `src/runs/client/cena-cabo`, de `fullscreen:true` e
`onboardAccessibility:false` no `options.txt`, e do `runGameTestClient` dentro de um `gamescope`. Os prints saem em
`src/runs/client/screenshots/`. Coordenada inteira no `tp` é centralizada: escreva `-2.0`.

| Caminho | Conteúdo |
|---|---|
| `~/.local/share/jogo-isolado/mekanism-ref/src` | Código do Mekanism |
| `~/.local/share/jogo-isolado/mekanism-ref/gradle` | Cache do Gradle, com o Minecraft e o NeoForge |
| `~/.local/share/jogo-isolado/mekanism-ref/src/runs/client` | Mundos e logs do jogo |
| `~/.local/share/jogo-isolado/mekanism-ref/src/runs/gameTestServer/gameTest` | Resultado dos testes, em JUnit |

## Comparar a tela

`comparar_tela.py` compara dois prints da janela inteira, do original e do port, tirados com o mesmo `guiScale` e do mesmo tamanho:

```sh
python3 -I -B comparar_tela.py original.png port.png --tela infuser --escala 2 --saida diferenca.png
```

Recorta a tela de 176 por 166 (centrada, ou onde `--origem x,y` disser), tira da conta as colunas das abas que o port não tem (a aba de
avisos, que ele tem, fica) e conta os pixels diferentes por elemento: slots, barras, setas, medidor, título, rótulo, inventário e o
fundo. `--margem N` inclui N pixels de GUI em volta, para ver as abas, e `--mascara x,y,largura,altura` tira mais uma área. O código
de saída é 0 sem diferença, 1 com alguma e 2 para entrada inválida. Telas: `electric`, `infuser` e `generator`. O alvo é zero.
Os testes (`test_comparar_tela.py`) rodam em `scripts/ci.sh`.

O sprite de químico do Infuser é animado nos dois jogos: compare com a barra vazia, ou mascare `7,15,6,54`.

### Todos os estados

`cena_telas.py` guarda a tabela de estados (máquina, energia, itens, químico, lava, passos do servidor, cursor). `cena_telas.py oficial
ARQUIVO ESCALA...` escreve o `cena-telas` que `testes/CenaTelas.java` lê (copie o Java para `src/gameTest/java/mekanism/common/tests/network/`
e o arquivo para `runs/client/`, tire o `cena-cabo` de lá, e rode `runGameTestClient` no gamescope). A cena deixa o servidor parado
(`tick freeze`), monta o estado, avança com `tick step`, abre a tela e tira o print; as abas que o port não tem ficam invisíveis no original, e o
mundo atrás é uma caixa de concreto preto. `cena_telas.py port DESTINO ESCALA...` faz o mesmo no Horizon, pelo `horizon-ctl`, e
`comparar_lote.py PASTA_ORIGINAL PASTA_PORT SAIDA ESCALA...` compara tudo e grava `tela_<caso>.png` (original, port e diferença) e um JSON.
Os cantos transparentes do painel mostram o mundo e contam à parte (`canto_*`). Os casos `ciclo` tiram 40 prints no original e `ciclo_port` no port (o ciclo inteiro da animação), um por passo, e cada print
do original tem de ser igual a algum do port.

