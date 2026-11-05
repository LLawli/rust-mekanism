# Plano de paridade 1:1

O objetivo é comportamento idêntico ao Mekanism e ao Mekanism Generators, branch 26.3 (`b2d04e166`, em
`/mnt/dados/caches/rust-mekanism-ref/mekanism`). O que já existe foi feito sob a decisão antiga de simplificar, e por isso é
ponto de partida, não entrega. Cada fase refaz sua parte a partir do Java.

## Regras de trabalho

1. **Java primeiro.** Leia a classe inteira e as que ela chama. Anote cada comportamento, inclusive ordem de execução no tick.
2. **Teste de referência antes do código.** Valores e sequências saem do Java, como `matches_the_reference_curve` do Heat
   Generator e o ejetor comparado tick a tick.
3. **Nomes e estrutura espelham o original** onde der: `Transmitter`, `DynamicNetwork`, `ConfigInfo`, `DataType`. Assim a
   comparação é linha a linha.
4. **Otimizações do original são portadas**, não trocadas por outras. Nada de cache por tempo.
5. **Bloqueio do Horizon:** tente injeção no jogo; se não der, registre na tabela abaixo e pergunte ao usuário.
6. **"Igual ao original" só se diz depois de comparar.** Modelo, textura e som: o mesmo arquivo, carregado do mesmo jeito.
7. **Testes sem o jogo primeiro:** a lógica roda num mundo simulado, tick a tick, comparando estados, sem ninguém interagir.
   Cenários como rede com cabos quebrados, chunks saindo, fusão de redes e receitas pausando devem caber aí.
8. **Em mundo, só o que falta:** visual, som e a integração com o Horizon. Superplano, peças colocadas como jogador (`place`,
   não `setblock`), prints e vídeo no WhatsApp por etapa.

Onde o código fica segue a regra do `CLAUDE.md`: mecânica reaproveitável vai para a `machinery_basis`, sem nada exclusivo do
Mekanism.

## Fase 0: verificar o que o Horizon permite

Sem código de jogo. O resultado preenche a tabela e decide o caminho das fases seguintes.

| Pergunta | Precisa para | Por onde começar |
|---|---|---|
| Ordem de `level_tick_post` e `server_tick` em relação ao tick das block entities | `TransmitterNetworkRegistry.onTick` roda em `ServerTickEvent.Post` | `game.rs`, hooks `tick`, `server_tick` |
| `chunk_unload` chega antes de a block entity sair? Há equivalente a `ChunkTicketLevelUpdatedEvent`? | Save shares, rede que atravessa chunks | `native_chunk_unload`, `native_chunks_changed` |
| Remoção e entrada de block entity no mundo | `onWorldJoin`, `setRemoved`, órfãos | Não há hook com `remove` no nome: procurar no `block_entities` |
| Quad translúcido com alfa variável por block entity | Conteúdo do cabo: `ARGB.white(scale)`, sheet translúcida, brilho total | `BlockEntityDraw`, `draw.quad`, `force_translucent`. `native_draw_quad` só recebe 0xRRGGBB |
| Bloco com camada translúcida ou cutout pelo modelo | Faixa transparente do braço (`x` de 12 a 19 na textura) | `mesher.rs`, `models/catalog.rs` |
| OBJ, ou partes de modelo por visibilidade | `transmitter_small.obj.mek`, grupos por lado | `standalone_models`, `native_draw_model`. Se não houver OBJ, converter fielmente as 24 partes |
| Estado por lado com quatro valores | `NONE`, `NORMAL`, `PUSH`, `PULL` e `NONE_CONTIGUOUS` | `state_properties` junto do template de chorus, ou modelo pela block entity |
| Luz por estado | `ACTIVE_LIGHT` é 8, `ACTIVE_MELT_LIGHT` é 13, e o template da fornalha dá 13 | Escrita da `emission` da lava em `set_state_properties` |
| Regra de slot no clique com acesso às receitas | `isItemValid` em clique | `native_menu_slot_rules` está fora de `crate::game`: estado compartilhado pelo `[[crate]]` core |
| Faces de energia, item, fluido por instância para outros mods | Capability por lado | `ModEnergyBuffer` é por tipo: procurar ponto de injeção em `native_energy_transfer` |
| Item guardando conteúdo ao quebrar | `mekanism:items`, `energy`, `chemicals`, `side_config` | `loot_drops`, `loot_function`, `StackData` |
| Som contínuo de máquina, com volume e fim | `ITileSound` | `native_play_sound`, loop |
| Comparador | `IComparatorSupport` | Não há hook com `comparator` no nome: procurar `block_signal` e injeção |
| Tooltip de item e do bloco, com shift para detalhes | `ItemBlockTooltip`, `DETAILS` | `tooltip`, `tooltip_component` |
| Tela com abas, botões e sub-janelas | Abas de energia, redstone, segurança, upgrades e configuração | `screen_*`, `native_screen` |
| Rodar o Mekanism real em NeoForge 26.3 para comparar | Referência de comportamento lado a lado | Perguntar ao usuário: precisa de cliente Java e conta |

## Referência e simulador

O Mekanism original compilado do commit de referência fica no desktop: `dev/referencia/README.md`. A Fase 1 começa criando na
`machinery_basis` um mundo simulado para testes, com blocos, block entities, ticks e chunks falsos atrás das mesmas regras que
o jogo chama. Onde der, gere as sequências esperadas do próprio Java: o Mekanism tem `gameTestServer`, que roda sem ninguém
interagir e pode produzir o estado tick a tick de um cenário, para o teste em Rust comparar.

## Fases

Cada fase depende da anterior. Ao fechar uma fase, atualize o inventário abaixo.

**Fase 1, base de tile.** Porte de `TileEntityMekanism`, `TileEntityConfigurableMachine`, `TileEntityRecipeMachine`,
`TileEntityProgressMachine` e `CapabilityTileEntity`: ordem do tick, `setActive` com seu atraso, controle de redstone, nome,
comparador, som, upgrades, sincronização de tela (`MekanismContainer`). Inclui `CachedRecipe` completo, com `OperationTracker`,
`pausedForErrors`, `recheckAllErrors`, erros e avisos da tela, e `usedSoFar` para `per_tick_usage`.

**Fase 2, lados coloridos.** Porte de `TileComponentConfig`, `ConfigInfo`, `DataType`, `ISlotInfo`, `TransmissionType` e
`TileComponentEjector`: por instância, por tipo de transmissão, com as cores, salvo no NBT, `AttachedSideConfig` como padrão.
Tela de lados, Configurator e os slots por `DataType` (`INPUT_1`, `INPUT_2`, `EXTRA`). Substitui `Sides`, `FaceSlots` e o
`machinery_basis/energy` atual.

**Fase 3, rede de transmissores.** Porte de `TransmitterNetworkRegistry`, `OrphanPathFinder`, `DynamicNetwork`,
`DynamicBufferedNetwork`, `Transmitter`, `BufferedTransmitter`, `AcceptorCache`, `NetworkAcceptorCache`, `EnergyNetwork`,
`UniversalCable`, `Target` e `SplitInfo`: buffer somado, save shares, fusão e divisão de rede, chunks, `ConnectionType`, pull,
redstone reativo, upgrade de tier no lugar, `currentScale` e `PacketNetworkScale`. Substitui `network.rs`, `transmit_energy` e o
cache de 20 ticks. A base genérica já precisa servir fluido, químico, item e calor.

**Fase 4, visual do cabo.** Modelo original com todas as partes, textura translúcida (não a `opaque/`), `hideContiguousJoin`,
conteúdo `mek_liquid/energy` animado com alfa pela escala, opção `opaqueTransmitters`, forma de seleção e colisão por lado
(`BlockSmallTransmitter`: `SIDES`, `CENTER`, flanges).

**Fase 5, refazer os blocos prontos sobre a base.** Heat Generator, Metallurgic Infuser, Enrichment Chamber, Crusher,
Energized Smelter e Steel Casing, um a um, conferindo o inventário. Telas com os sprites e widgets originais, energia em slot,
upgrades, conteúdo guardado no item, som, partículas e luz. Sai a bateria de teste `mek_teste` do build.

**Fase 6, seguir a cadeia com paridade.** Energy Cube e Energy Tablet, Solar Generator, minérios de estanho, chumbo, urânio e
fluorita, tubos, canos e transportadores sobre a rede da Fase 3.

## Inventário do que diverge hoje

**Rede e cabo**

- Sem objeto de rede: buffer só nos cabos de entrada, em vez da soma da rede. Aceita menos por tick.
- Sem save shares: quebrar um cabo de entrada perde a energia dele.
- Travessia guardada por 20 ticks: mudanças levam até 1 s, e nesse tempo a energia pode entrar por uma face errada.
- Sem `ConnectionType`, pull, redstone reativo e upgrade no lugar.
- Modelo em JSON aproximado, textura `opaque/`, sem faixa translúcida, sem verde animado, sem flange, emenda visível.
- Faces declaradas por tipo de bloco; o original responde pela capability por lado e por instância.

**Máquinas (infuser e elétricas)**

- Faces fixas, sem tela de lados, sem ejetor e sem automação da saída.
- `CachedRecipe` simplificado: sem `pausedForErrors`, erros, avisos e `recheckAllErrors`. `per_tick_usage` ausente.
- Clique põe qualquer item na entrada. Slot de energia sem função.
- Sem upgrades, controle de redstone, comparador, som, partículas, abas, nome do químico no tooltip.
- Luz 13 em vez de 8. Conteúdo solto ao quebrar, em vez de guardado no item.
- Barras desenhadas com retângulo, não com os sprites do Mekanism.
- `bio_fuel` sem tempo de queima. Tags `c:` escritas à mão: conferir com o NeoForge da versão.

**Heat Generator**

- Temperatura do bioma sem ajuste por altura.
- Lava dentro do bloco desenhada pela block entity; entidade ali dentro não conta como dentro de lava.
- Sem slot de energia funcional, transferência de calor pela base, som, partículas, abas, tooltips e ícone de raio.
- Tanque de lava não recebe de cano. Slots da tela 1 pixel deslocados.

## Pronto significa

Uma parte só sai do inventário quando: o Java foi lido por inteiro, o teste de referência passa, o cenário foi repetido em
mundo, e o que a pessoa vê e ouve foi comparado com o original. Se a referência em NeoForge existir, com o mesmo cenário nos
dois jogos.
