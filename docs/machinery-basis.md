# machinery_basis: escopo

Biblioteca genérica para mods de máquina no Horizon. Nada nela é exclusivo do Mekanism: ela não conhece hidrogênio nem
Metallurgic Infuser. Um mod de magia usa os mesmos tanques para mana, e o Mekanism registra químicos como dados.

Levantado em 2026-11-05. Pin: Nightly `f319701` (API 409), a primeira com `native_container_accepts` e
`native_menu_extra_data`. O mod de teste compilou nela sem mudanças. A pesquisa em `main` foi feita no commit `85d8e28`.

## Regra de escopo

Entra na lib o que atende às três condições:

1. Dois mods sem relação entre si precisariam disso.
2. O Horizon não oferece, ou oferece com um limite que a lib resolve.
3. Um consumidor real vai usar agora.

A terceira segura o motor de multibloco: ele é parte do plano, mas só nasce com a primeira estrutura real.

## Decisão por item

**Usar como está:** a lib não embrulha, e o mod chama o Horizon direto.

| Item | No Horizon | Observação |
|---|---|---|
| Registrar blocos, itens e menus | Hooks `blocks`, `items`; `data/<id>/horizon/menu/` | |
| Block entity, tick e salvamento | `block_entity`, `block_entity_ticks`, `block_entity_tick` | Salvamento testado em mundo |
| Slots de item e funis | `ModContainer`, `native_insert_item`, `native_extract_item`, `native_move_items` | Interopera com hoppers |
| Buffer de energia | `ModEnergyBuffer`, `native_energy_insert` | Testado em mundo. `i32` cobre o maior cubo não criativo (256.000.000) |
| Energia em itens | `item_energy_buffer` | Para o `energy_tablet` |
| Tanques de fluido | `ModFluidTanks`, `native_move_fluids` | Até 16 tanques de 1.000.000 mB; o Fluid Tank Ultimate tem 256.000 |
| Fluido em itens | `native_item_fluid_tank` | |
| Estado do bloco | `state_properties`, `native_set_block_property` | `facing`, `active` |
| Redstone | `neighbor_signal`, `native_update_signal_neighbors` | |
| Manter chunk carregado | `native_force_chunk` | Tickets não são salvos: guarde e reaplique |
| Rede cliente e servidor | `native_net_send_to_player`, `native_net_send_to_server` | |

**Embrulhar:** existe, mas tem uma armadilha que todo mod repetiria.

| Item | Problema | A lib oferece |
|---|---|---|
| Distribuir energia | O bloco em tick sai do mundo: `native_energy` responde `None` e `native_move_energy` não esvazia o próprio bloco | `distribuir_energia(game, pos, entity, faces)` |
| Data slots | Trafegam em 16 bits | Codec que divide `i32` e `i64` em slots |
| Receitas de tipo próprio | `native_recipes_of_type` entrega JSON cru, sem busca | Índice por tipo, refeito em `server_data_loaded` |
| Telas | `native_screen` e `native_sprite` são primitivas | Barra de energia, medidor de tanque, seta de progresso, botões |
| Faces relativas | `faces` são absolutas e por tipo de bloco, e a máquina gira | Conversão de lado relativo para direção, pelo `facing` |

**Reimplementar:** o Horizon não tem, ou não serve para a meta.

| Item | Por quê | Tamanho no Mekanism |
|---|---|---|
| Recursos tipados e seus tanques | Químico como fluido não dá paridade: o Chemical Tank Elite tem 1.024.000 mB e o Ultimate 8.192.000, acima do limite de 1.000.000. Um mod de fluidos também trataria hidrogênio como líquido | 1.519 linhas (`api/chemical`) |
| Lados configuráveis por máquina | Faces do Horizon são por tipo de bloco | Parte de 2.449 (`tile/component`) |
| Ejetor | Empurra saídas conforme a configuração de lados | Parte de 2.449 (`tile/component`) |
| Base de máquina | Progresso, operação, consumo por tick e controle de redstone | 1.589 (`tile/prefab`) |
| Redes de transporte | `connects_to` só dá a forma do cabo, não a rede | 5.172 (`lib/transmitter`, `content/network`) |
| Motor de multibloco | Não existe | 2.614 (`lib/multiblock`) |

### Lados configuráveis

A máquina declara `inputs` e `outputs` vazios, então hopper nenhum a enxerga. Quem age é ela, imitando o hopper e
respeitando as faces configuradas. Assim a regra vale por instância e acompanha a rotação.

| Situação | O que a máquina faz |
|---|---|
| Hopper logo abaixo, com a face de baixo como saída | Entrega um item, mesmo com o push desligado, porque um hopper puxa do bloco acima |
| Hopper em outra face | Não entrega: hopper só puxa de cima |
| Hopper apontando para a máquina, com aquela face como entrada | Recebe um item, como se ele entregasse |
| Hopper apontando para outro lugar | Ignora |
| Hopper travado por redstone (`enabled=false`) | Ignora |

O ritmo é o do hopper: 1 item a cada 8 ticks por elo, e só um sucesso espera. O push ligado na configuração é outra coisa:
entrega ativa a qualquer inventário que aceite entrada, pelas faces de saída, hopper incluído, como um dropper atirando
num hopper. O ritmo é o do ejetor do Mekanism: dispara a cada 11 ticks, tenha saído algo ou não.

Limites: outros mods que automatizam pelas faces do Horizon também não enxergam a máquina, e fluido ou energia vindos
deles seguem as faces fixas do tipo de bloco. Hopper minecart ainda não entra na regra. O elo tem contador próprio, sem
sincronizar com o cooldown interno do hopper, que os helpers não expõem.

### Fluido dentro do bloco

Um bloco de qualquer mod guarda lava declarando `fluid_logged` nos `state_properties`, com os valores `empty` e `lava`.
A lib cuida do balde, do fluxo, da luz e do desenho.

| Parte | Como |
|---|---|
| Fonte de lava real | Injeção em `set_state_properties`, que escreve lava na linha do estado na tabela do motor de fluidos. O Horizon só reconhece fluido em bloco para água. É um alvo interno, preso à versão do jogo |
| Balde | Hooks `use_item_on` e `use_item`: balde de lava enche, balde vazio tira. Um bloco com tela só recebe o balde com shift |
| Fluxo | Hook `update_shape`: um vizinho mudou, a lava olha de novo para onde correr |
| Luz | Nível 15 no estado com lava |
| Desenho | A block entity desenha a lava (hook `block_entity_submit`), e o bloco precisa responder `block_entity_update_tag` |

A lava respeita a forma do bloco, como no vanilla: não passa por uma face toda fechada. O Heat Generator tem a base sólida,
então a lava sai pelas laterais e nunca por baixo. O atraso do fluxo é o do Overworld, 30 ticks, também no Nether.

Limites: o mesher, a física de entidades e a colisão de fluido leem `FluidState::from_block`, que nenhuma injeção alcança
na versão fixada, porque o corpo usa `?` e o retorno usa `Self`. Por isso um bloco sem block entity flui mas não mostra a
lava, e uma entidade dentro do bloco não conta como dentro de lava. Um estado com lava posto por comando ou pela geração do
mundo só corre quando um vizinho muda, porque só o balde agenda o fluxo.

## Fora da lib

Fica no mod que usa: ids de químicos, receitas, tiers, fábricas, texturas e o layout de cada tela. Também ficam de fora
upgrades específicos, configurador, segurança por dono, QIO e radiação, até dois mods precisarem deles.

## Arquitetura

Um mod `machinery_basis` com três partes, cada uma com um papel que os testes confirmaram:

| Parte | Tipo | Conteúdo | Quem alcança |
|---|---|---|---|
| `core/` | `[[crate]]` | Regras puras, sem tipos do jogo, e os registros de recursos | Mixin e `[api]`, uma cópia só |
| `glue.rs` | `[[module]]` | Tudo que chama o jogo | Só mixin, como `crate::game::machinery_basis` |
| `api/` | `[api]` | `pub use` do `core` e a dependência | Mods tipados, como `machinery_basis::core` |

O `core` guarda quase toda a lógica e testa com `cargo test`, sem jogo. O `[api]` alcança o `core` por um caminho que só
existe no build do launcher, `../../native-crates/machinery_basis/core`, então o `cargo test` local roda só no `core`.

Limite atual: um mod só tipado usa o `core`, mas não o `glue.rs`, porque o `Host` não tem energia. Esse mod precisa de
mixin para mover energia até o trabalho de [capabilities](https://github.com/horizon-rust/horizon/issues/7387) chegar.
Quando elas chegarem, boa parte do `glue.rs` deve encolher, por isso a lib mantém o código do jogo isolado ali.

## Ordem

1. **`core`:** recursos, tanque, lados, progresso de máquina, codec de data slots e índice de receitas.
2. **`glue.rs`:** salvar tanques no NBT, distribuir energia e ejetar.
3. **Telas:** barra, medidor e seta, com a primeira tela real.
4. **Rede:** cabo de energia, depois tubo de fluido e de químico.
5. **Multibloco:** com a primeira estrutura do Mekanism, como o Dynamic Tank.

Os consumidores das etapas 1 a 3 são o Heat Generator e o Metallurgic Infuser. Um consumidor que não seja de máquina,
como o caso do Ars Nouveau, é o teste de que a lib continua genérica.

## Decisões

1. **Registro de recursos por dados:** cada recurso é um arquivo em `data/<ns>/machinery_basis/resource/<nome>.json`, lido
   pelo `native_add_server_reload_listeners` (API 400). O id vira `<ns>:<nome>`. Isso vale para mods de conteúdo e para
   data packs do mundo, e o `/reload` relê. A leitura só existe no servidor: o cliente recebe o que precisa pela rede.
2. **Repositório:** a lib fica neste repo por enquanto, para mudar junto com o primeiro consumidor.

3. **Formato dos tanques:** versionado desde o início, com `v: 1` no NBT, em `machinery_basis.tanks`. É o contrato entre
   mods. Uma versão desconhecida carrega vazia.

## Verificado

| Teste | Resultado |
|---|---|
| Mod de teste na Nightly `f319701` | `native.py check` OK |
| `[api]` usando `[[crate]]` pelo caminho do stage, com outro mod tipado chamando | `native-build` OK, sem rodar em mundo |
| Gerador e bateria em mundo, com salvamento | OK, ver [modding-horizon.md](modding-horizon.md) |
| Recursos por dados em mundo | Mod de conteúdo lido ao abrir; data pack do mundo entrou no `/reload`; arquivo `.txt` ignorado |
| Lib real com consumidor separado, em mundo | Energia, hoppers, ejetor, tanque e recursos funcionam; 85 itens entraram e 85 chegaram aos baús |
| Tanque depois de fechar e reabrir | Preservado, com 100 de hidrogênio |
| Arquivo de recurso sem cor | Recusado com a mensagem certa, sem afetar os outros |
| Regra de hoppers em mundo, com a máquina sem interface vanilla | Entrega ao hopper de baixo a 1 item por 8,13 ticks; hoppers de lado, com a face bloqueada, apontando para outro lugar ou travados não foram tocados |
| Lava dentro do bloco, em mundo | Balde com shift enche, a lava corre com níveis 2, 4 e 6 pelas quatro laterais e nada sai pela base; balde vazio tira e ela recua; parede removida com a lava parada abre o fluxo; quebrar com picareta deixa `lava[level=0]`; luz 15 com o bloco cercado de vidro; sem shift só abre a tela |

Lido, não executado: redes, telas, receitas de tipo próprio, `native_force_chunk` e `native_container_accepts`.

Os ritmos têm teste puro no `core`, sem o jogo: hopper a exatos 8 ticks, ejetor a exatos 11, e o ejetor comparado tick a
tick com o laço do `TileComponentEjector` por 1.000 ticks.
