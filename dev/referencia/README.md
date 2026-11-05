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
