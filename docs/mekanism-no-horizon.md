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
| Cabos | `connects_to` e `thickness` dão a forma | Coberto: a rede é da `machinery_basis`; falta o item e o fluido |
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

- as faces são as padrão, sem configuração. A saída fica à direita e um hopper só puxa de cima, então ela sai à mão. Hopper
  não aponta para cima, então o slot de químico da base só entra pela tela;
- sem upgrades, controle de redstone, ejetor, tubo de químico, som, abas e o nome do químico no tooltip;
- o slot de energia aceita `redstone` e `redstone_block` e converte como o original (`fillContainerOrConvert`), mas a conversão só
  vale se tudo couber: `energy_conversion` dá 10.000 e 90.000, e o buffer da máquina de um jogador sem upgrade tem 1.000 (1.600 no Crusher; 20 na do `setblock` ou lida do disco sem upgrade, e até 10.000 com 8 de velocidade, 16.000 no Crusher), então a `redstone` só converte com velocidade 8 (7 ou 8 no Crusher) e o contêiner vazio: nas outras o
  item entra, fica no slot e a energia não sobe, nos dois;
- a luz da máquina ligada é 13, do template da fornalha, e não 8;
- quebrar o bloco solta o conteúdo, em vez de guardar no item;
- `per_tick_usage` é lido e o Infuser escolhe `ItemChemicalConstantUsageRecipe` quando é verdadeiro, mas nenhuma receita de infusão dos arquivos usa; a Chemical Injection Chamber vai precisar.

## Máquinas elétricas: estado atual

Enrichment Chamber, Crusher e Energized Smelter são uma tabela só em `native/glue.rs`, porque no original as três são
`TileEntityElectricMachine` e só mudam em id, receitas e consumo: 5, 8 e 5 J por tick, com 200 ticks e buffer de quatro
ticks de uso. O Smelter não tem arquivos de receita próprios: usa as receitas de fornalha do jogo, como o `WrappedSmelterRecipe`.

As 460 receitas de `mekanism:enriching` e `mekanism:crushing` são os arquivos originais. As tags `c:` do NeoForge que o vanilla
não tem foram escritas à mão em `data/c/tags/item/`, com os mesmos itens. Ficam dormentes as receitas de estanho, chumbo,
urânio, fluorita, sal e o que vem do processamento de minério em mais etapas, até esses itens existirem.

Testado em mundo: craft das quatro peças, minério bruto em pó (3 para 4), carvão em `enriched_carbon`, ferro em pó,
pedregulho em cascalho, pó em lingote, `enriched_carbon` dando 80 mB de carbono no infuser, hoppers por face e item sem
receita recusado por hopper e por shift-click.

Têm as diferenças do infuser que valem para máquina: faces fixas, sem upgrades, sem controle de redstone, sem slot de energia
funcional, luz 13 em vez de 8 e conteúdo solto ao quebrar. Além disso, o `bio_fuel` não queima como combustível.

## Universal Cable: estado atual

Os quatro tiers guardam 8.000, 128.000, 1.024.000 e 8.192.000 J (`CableTier`), e tiers diferentes se ligam. Como no original,
um cabo só se liga onde há troca de energia, e entrada é diferente de saída. Cada bloco declara seus lados em
`data/<ns>/machinery_basis/energy/<bloco>.json`: o Heat Generator tem saída só pela frente, uma máquina tem entrada por todos
os lados, e uma bateria pode ter os dois. Um lado de saída liga o cabo, mas a rede não entrega por ele.

Testado em mundo: um gerador alimentando três máquinas pela mesma rede; com energia escassa, 925, 928 e 930 J para cada
uma em 300 ticks; cortar o cabo pausa as máquinas sem perder progresso; religar com outro tier volta em menos de um segundo;
a energia do cabo sobrevive a reabrir o mundo; craft do básico e do avançado.

Ainda difere do original em:

- a energia fica no cabo que a recebeu, não num buffer somado da rede. No original a capacidade soma todos os cabos, então
  aqui a rede guarda e aceita por tick só o que os cabos de entrada comportam, e quebrar um deles perde o que ele tinha;
- uma mudança na rede leva até um segundo para valer, porque a travessia é guardada por 20 ticks;
- o modelo é uma aproximação em JSON, não o OBJ original: sem a parte translúcida do braço, sem o conteúdo verde animado
  (`mek_liquid/energy`, com alfa pelo nível da rede), sem o flange na ligação com máquina e sem esconder a junção em trechos
  retos. A textura usada no braço é a variante `opaque/`;
- sem modos de conexão por lado (normal, push, pull, none), controle por redstone e upgrade de tier no lugar.

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
4. **Fidelidade:** paridade 1:1 com o original, decidida pelo usuário em 2026-11-05. Substitui a decisão anterior de começar
   com uma versão simplificada. Nada se decide por simplicidade ou desempenho. Plano em
   [plano-paridade.md](plano-paridade.md).

5. **Valores de jogo:** sempre os padrões da versão mais nova do Mekanism, a branch 26.3. A wiki e os valores que muita
   gente lembra (200 J/t, tanque de 24 baldes, 10 mB/t) são da 1.21.x e anteriores. A 1.21.11 reescalou energia e queima de
   todo o mod, então misturar as duas versões desequilibra o jogo.

## Janela de upgrades: o que o Horizon faz diferente

- **Sincronização.** No Java, `upgradeTicks` e o mapa de upgrades só viajam para quem abriu a janela (`startTracking` por jogador). Os
  data slots do Horizon são por bloco, então o menu leva 9 data slots a mais (`upgradeTicks` e uma contagem por tipo, na ordem do
  registro) e o servidor só escreve valores neles enquanto algum jogador acompanha (`CONTAINER_TRACK_UPGRADES` sem `STOP`); no resto do
  tempo eles ficam em zero e não geram tráfego. Diferença visível: nenhuma. Diferença de tráfego: com a janela aberta para um jogador,
  todos os que olham o mesmo bloco recebem os 9 valores quando mudam, e o Java mandaria só a quem abriu. Um segundo `TRACK` sem `STOP`
  deixa o acompanhamento ligado depois do `STOP`, como o contêiner do Java faz (rastros `*_container_tracking`).
- **Ordem da lista.** O Java lista `Object2IntOpenHashMap.keySet()`, ordem de hash de objetos sem `hashCode` próprio, ou seja, não
  definida. A lista aqui segue a ordem do registro (por id).
- **Dados do cliente.** Os tipos e as tags de upgrade são lidos onde a carga de dados roda, e um cliente de outro processo (convidado
  de LAN, servidor dedicado) não a faz. No original os tipos são um registro sincronizado. Aqui o servidor manda
  `upgrade::encode_sync` (tipos e tags, JSON) por `native_net_send_to_player`, no canal `machinery_basis:upgrade_types`, quando o
  jogador entra (`player_logged_in`) e a cada carga de dados (`server_data_loaded`, o `/reload`); o cliente guarda por
  `net_from_server` na fonte `sync`. `datapack_sync` não serve: ele leva só receitas. Medido em jogo com dois clientes (2026-11-06): o
  convidado vê a lista e os ícones de "Supported:". Limite: a fonte `sync` fica até o próximo envio, então um cliente que sai de um
  servidor e abre um mundo local carrega os tipos do servidor antigo até a primeira carga local (os tipos vêm dos mesmos arquivos do
  mod). Os 9 data slots do `upgradeTicks` e do mapa ficam: o envio por jogador só pouparia o tráfego de quem não abriu a janela.
- **Um tick de atraso.** O cliente do original mostra o progresso de instalação de um tick atrás (medido: o print do original com 10
  passos é igual ao do port com 9). O port escreve nos data slots o valor do tick anterior (`upgrade_sync_seen`).

## Lib compartilhada

`machinery_basis` é um terceiro mod genérico, sem nada exclusivo do Mekanism, para outros mods usarem: tanques tipados,
faces configuráveis, rede de transporte e, mais tarde, motor de multibloco. Os químicos do Mekanism entram como dados
registrados nela.

A cadeia é Generators, depois Mekanism, depois `machinery_basis`, sempre com uma única cópia da lib em execução. Cada mod
é um bundle híbrido: `[api]` e `[dependencies]` para o launcher validar a cadeia e para expor a API, e mixin no que o
`Host` ainda não oferece. Detalhes em
[Compartilhar código entre mods nativos](modding-horizon.md#compartilhar-código-entre-mods-nativos).
