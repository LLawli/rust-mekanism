# Mekanism de referência

O mod original, compilado do commit que o port segue, para comparar comportamento lado a lado. Roda no mesmo isolamento do
Horizon: é código de terceiros, e o `runClient` não usa conta.

```sh
./instalar <repositório do Mekanism>   # precisa do jogo-horizon já construído
mekanism-ref offline                   # compila na primeira vez e abre o jogo
JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon compileJava' mekanism-ref
```

| Caminho | Conteúdo |
|---|---|
| `~/.local/share/jogo-isolado/mekanism-ref/src` | Código do Mekanism |
| `~/.local/share/jogo-isolado/mekanism-ref/gradle` | Cache do Gradle, com o Minecraft e o NeoForge |
| `~/.local/share/jogo-isolado/mekanism-ref/src/runs/client` | Mundos e logs do jogo |
