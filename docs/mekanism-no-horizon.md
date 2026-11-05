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
| Químicos (`#mekanism:redstone`, `#mekanism:carbon`) | Não existe | Recursos e tags da `machinery_basis`, com o tanque no NBT da máquina |
| Lados configuráveis por máquina | Faces fixas por tipo de bloco | Lacuna (abaixo) |

## Pontos que mudam o desenho

**O infuser depende de químicos.** O carbono e a redstone do Metallurgic Infuser são químicos, não itens, e entram por
receitas `mekanism:chemical_conversion`. O Horizon não tem esse sistema, então cada químico é um recurso da `machinery_basis`
e a máquina guarda um tanque no NBT. Os tubos de químicos podem esperar.

**Lados configuráveis.** No Mekanism, cada máquina escolhe o que entra e sai por cada face. No Horizon, as faces são
declaradas uma vez por tipo de bloco e são direções absolutas. `native_container_accepts` permite recusar itens por
posição e face, o que resolve a entrada de itens; ele existe a partir da Nightly `f319701`. Saída, energia e fluidos por
instância ficam com os canos e ejetores da `machinery_basis`. Para o primeiro marco, faces fixas são suficientes.

**O worldgen precisa ser reescrito.** Os arquivos usam `mekanism:ore`, `mekanism:disableable`,
`mekanism:configurable_constant` e `mekanism:configurable`, que dependem de código Java. É preciso trocá-los pelos tipos
`minecraft:` e fixar os valores. O biome modifier pode ser aproveitado, mas ele aponta para a tag `#mekanism:spawn_ores`,
que precisa ir junto.

**Números maiores que 16 bits na tela.** Data slots têm 16 bits, e energia passa disso. Cada valor grande precisa de dois
slots.

**Mixin agora, API tipada depois.** O Host da API tipada não tem os helpers de energia, e o registro ainda não aceita
`[api]`. Seguir o padrão do Deepbore: helpers mínimos, `glue.rs` com os tipos do jogo e regras puras testáveis. A lógica
pesada pode ir para um `[[crate]]`, que compila uma vez só.

## Heat Generator: estado atual

Segue a simulação de calor oficial (`TileEntityHeatGenerator`). A curva bate com a referência tirada do Java, e os valores
são os padrões do código atual: tanque de 1.000 mB, 100 mB por tick, buffer de 240.

Funciona em mundo: modelo, forma real de colisão, receita, combustíveis de fornalha (viram lava pela proporção do tempo de
queima), geração passiva por lados com lava, lava dentro do bloco com shift + balde, que corre pelas laterais como lava
real, energia pela frente, hoppers e tela.

Ainda difere do original em:

- temperatura do bioma sem o ajuste por altura;
- lava dentro do bloco desenhada pela block entity, e uma entidade ali dentro não conta como dentro de lava (limites em
  [machinery-basis.md](machinery-basis.md#fluido-dentro-do-bloco));
- sem slot de energia funcional, transferência de calor pela face de baixo, som, partículas, abas e tooltips da tela;
- tela sem os sprites finais do Mekanism e sem o ícone de raio;
- lava num tanque da lib: não recebe de cano de fluido.

## Metallurgic Infuser: estado atual

Segue `TileEntityMetallurgicInfuser` e `CachedRecipe`, com os valores da 26.3: 200 ticks, 5 J por tick, buffer de 20 J e
tanque de 1.000 mB. As receitas `mekanism:metallurgic_infusing` e `mekanism:chemical_conversion` são os arquivos originais.
Uma receita que cita um item que ainda não existe fica de fora sozinha e entra quando o item chegar.

Funciona em mundo: modelo, forma real, receita da máquina, tela com slots, barras, seta e botão de esvaziar, shift-click para
o slot certo, conversão de item em químico, processamento, energia por todas as faces e hoppers por face. Sem energia ou
com a saída cheia o progresso espera; sem entrada, ou com outra receita, volta a zero. Progresso, tanque e receita
sobrevivem a fechar e reabrir o mundo.

Os itens de agora são `enriched_iron`, `dust_steel`, `ingot_steel`, `basic_control_circuit` e `alloy_infused`. Dos oito
químicos de infusão, carbono, redstone e fungi já têm item de origem.

Ainda difere do original em:

- um clique do jogador põe qualquer item nos slots de entrada: `native_menu_slot_rules` roda fora de `crate::game` e não
  alcança as receitas. Hoppers e shift-click respeitam a regra, e a saída recusa item;
- as faces são as padrão, sem configuração. A saída fica à direita e um hopper só puxa de cima, então ela sai à mão. Hopper
  não aponta para cima, então o slot de químico da base só entra pela tela;
- sem upgrades, controle de redstone, ejetor, tubo de químico, som, abas e o nome do químico no tooltip;
- sem slot de energia funcional. Redstone não serviria: `energy_conversion` dá 10.000 J, e o original só aceita se tudo
  couber, o que nunca acontece num buffer de 20 J;
- a luz da máquina ligada é 13, do template da fornalha, e não 8;
- quebrar o bloco solta o conteúdo, em vez de guardar no item;
- `per_tick_usage` não é lido: nenhuma receita de infusão usa, mas a Chemical Injection Chamber vai precisar.

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

5. **Valores de jogo:** sempre os padrões da versão mais nova do Mekanism, a branch 26.3. A wiki e os valores que muita
   gente lembra (200 J/t, tanque de 24 baldes, 10 mB/t) são da 1.21.x e anteriores. A 1.21.11 reescalou energia e queima de
   todo o mod, então misturar as duas versões desequilibra o jogo.

## Lib compartilhada

`machinery_basis` é um terceiro mod genérico, sem nada exclusivo do Mekanism, para outros mods usarem: tanques tipados,
faces configuráveis, rede de transporte e, mais tarde, motor de multibloco. Os químicos do Mekanism entram como dados
registrados nela.

A cadeia é Generators, depois Mekanism, depois `machinery_basis`, sempre com uma única cópia da lib em execução. Cada mod
é um bundle híbrido: `[api]` e `[dependencies]` para o launcher validar a cadeia e para expor a API, e mixin no que o
`Host` ainda não oferece. Detalhes em
[Compartilhar código entre mods nativos](modding-horizon.md#compartilhar-código-entre-mods-nativos).
