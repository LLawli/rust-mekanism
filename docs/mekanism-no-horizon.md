# Mekanism no Horizon: o que a API cobre

Cruza o começo do jogo do Mekanism com o que o Horizon oferece. A explicação geral de mods está em
[modding-horizon.md](modding-horizon.md).

Referência: [mekanism/Mekanism](https://github.com/mekanism/Mekanism), branch padrão, versão 10.8.0, que já mira o
Minecraft 26.3, a mesma versão do Horizon. Por isso os dados gerados (`src/datagen/generated/`) estão no formato certo,
tirando os tipos próprios do Mekanism. O código é Java para NeoForge e precisa ser reescrito.

## A cadeia do começo do jogo

Tirada das receitas do repositório:

1. **Ósmio:** minério, que vira lingote na fornalha.
2. **Heat Generator** (`mekanismgenerators:heat_generator`): ferro, tábuas, ósmio, cobre e fornalha. É o primeiro
   gerador possível, porque não pede nenhum item processado por máquina.
3. **Metallurgic Infuser:** ferro, fornalha, redstone e ósmio. Também só pede itens básicos.
4. **Aço:** ferro + carbono no infuser dá `enriched_iron`; mais carbono dá `dust_steel`; a fornalha dá o lingote.
5. **Circuito e liga:** ósmio + redstone no infuser dá `basic_control_circuit`; cobre + redstone dá `alloy_infused`.
6. **Steel Casing:** aço, vidro e ósmio. É a base das máquinas seguintes.
7. **Máquinas elétricas:** Enrichment Chamber, Crusher e Energized Smelter.
8. **Solar Generator:** painéis, liga, ferro, ósmio e `energy_tablet`. Vem mais tarde que o Heat Generator.

Sugestão de primeiro marco: os passos 1 a 3, porque exercitam mundo, energia, menu e receita própria com poucos blocos.

## Cobertura

| O Mekanism precisa de | No Horizon | Situação |
|---|---|---|
| Minério no mundo | `configured_feature`, `placed_feature` e biome modifier | Coberto, com reescrita (abaixo) |
| Blocos e itens | Arquivos de conteúdo ou hooks `blocks` e `items` | Coberto |
| Receitas de crafting e fornalha | Formato vanilla | Coberto; as tags `c:` também são aceitas |
| `mekanism:enriching`, `crushing` etc. | `native_recipes_of_type` lê os arquivos do `.hmod`; a máquina aplica | Coberto; o JEI e o livro de receitas ficam de fora |
| Energia | `ModEnergyBuffer`, em `i32` | Coberto; o maior cubo não criativo (256.000.000) cabe em `i32` |
| Tanques de fluido | `ModFluidTanks` | Coberto para tanques; sem bloco nem balde de fluido de mod |
| Menus e telas | Tipo de menu próprio, `native_screen`, `native_sprite` | Coberto |
| Cabos | `connects_to` e `thickness` dão a forma | Parcial: a rede é código nosso |
| Químicos (`#mekanism:redstone`, `#mekanism:carbon`) | Não existe | Código nosso, guardado no NBT da máquina |
| Lados configuráveis por máquina | Faces fixas por tipo de bloco | Lacuna (abaixo) |

## Pontos que mudam o desenho

**O infuser depende de químicos.** O carbono e a redstone do Metallurgic Infuser são químicos, não itens, e entram por
receitas `mekanism:chemical_conversion`. O Horizon não tem esse sistema. Para o começo do jogo basta um tanque interno
no NBT da máquina. Os tubos de químicos podem esperar.

**Lados configuráveis.** No Mekanism, cada máquina escolhe o que entra e sai por cada face. No Horizon, as faces são
declaradas uma vez por tipo de bloco e são direções absolutas. `native_container_accepts` permite recusar itens por
posição e face, o que resolve a entrada de itens, mas ele só existe em `main`: não está na Nightly de 2026-11-04 nem no
catálogo. Saída, energia e fluidos por instância ainda precisam de estudo. Para o primeiro marco, faces fixas são
suficientes.

**O worldgen precisa ser reescrito.** Os arquivos usam `mekanism:ore`, `mekanism:disableable`,
`mekanism:configurable_constant` e `mekanism:configurable`, que dependem de código Java. É preciso trocá-los pelos tipos
`minecraft:` e fixar os valores. O biome modifier pode ser aproveitado, mas ele aponta para a tag `#mekanism:spawn_ores`,
que precisa ir junto.

**Números maiores que 16 bits na tela.** Data slots têm 16 bits, e energia passa disso. Cada valor grande precisa de dois
slots.

**Mixin agora, API tipada depois.** O Host da API tipada não tem os helpers de energia, e o registro ainda não aceita
`[api]`. Seguir o padrão do Deepbore: helpers mínimos, `glue.rs` com os tipos do jogo e regras puras testáveis. A lógica
pesada pode ir para um `[[crate]]`, que compila uma vez só.

## Licença e créditos

O Mekanism é MIT (Copyright 2017-2025 Aidan C. Brady), o que permite o port e cumpre a política do registro, desde que:

- o aviso de copyright e a licença sigam junto com o que for derivado;
- a página do mod credite os autores, aponte o original e deixe claro que não é um projeto oficial.

O arquivo `LICENSE` não separa código e assets, e o README diz que o mod todo é MIT. Antes de publicar texturas, vale
confirmar com os autores: é um risco baixo e evita um pedido de remoção. O port do Create não distribui os assets
porque os dele são All Rights Reserved, o que não parece ser o caso aqui.

## Decisões

Tomadas em 2026-11-05:

1. **Id do mod:** `mekanism`. Mantém ids e receitas originais sem renomear nada. Com outro id, blocos e itens virariam
   `<id>:*`, porque só podem ser declarados em `data/<id>/horizon/`, e o campo `namespaces` não resolve isso.
2. **Mods separados:** Mekanism e Mekanism Generators, como no original.
3. **Licença:** MIT, com os avisos do original em `THIRD_PARTY_NOTICES.md`.
4. **Fidelidade:** versão simplificada no começo, com paridade exata como meta. Cada simplificação fica registrada como
   diferença conhecida, para não virar comportamento definitivo.

## Lib compartilhada

`machinery_basis` é um terceiro mod genérico, sem nada exclusivo do Mekanism, para outros mods usarem: tanques tipados,
faces configuráveis, rede de transporte e, mais tarde, motor de multibloco. Os químicos do Mekanism entram como dados
registrados nela.

A cadeia é Generators, depois Mekanism, depois `machinery_basis`, sempre com uma única cópia da lib em execução. Cada mod
é um bundle híbrido: `[api]` e `[dependencies]` para o launcher validar a cadeia e para expor a API, e mixin no que o
`Host` ainda não oferece. Detalhes em
[Compartilhar código entre mods nativos](modding-horizon.md#compartilhar-código-entre-mods-nativos).
