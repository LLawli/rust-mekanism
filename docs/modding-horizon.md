# Como funciona a criação de mods no Horizon

O Horizon é a reescrita em Rust do cliente e da lógica do Minecraft: Java Edition 26.3 (MIT,
[horizon-rust/horizon](https://github.com/horizon-rust/horizon)). Jars de Fabric, Forge e NeoForge não rodam nele,
porque o código desses mods chama classes Java que o Horizon não tem. Do mod Java só aproveitamos dados e assets.

Levantado em 2026-11-05, sobre o commit `85d8e28` de `main` do Horizon. A API muda todo dia, então confira os números
antes de confiar neles. A seção [O que foi verificado](#o-que-foi-verificado) separa o que foi executado do que foi
só lido.

## Os dois tipos de mod

| | Mod de conteúdo | Mod nativo |
|---|---|---|
| O que é | Dados e assets: blocos, itens, receitas, loot, tags, modelos, texturas, worldgen | Código Rust compilado dentro do jogo |
| Arquivo | `.hmod` | `.hnmod` |
| Manifesto | `horizon-mod.toml` | `horizon-mixin.toml` |
| Quando age | O jogo lê ao iniciar | O launcher compila um jogo próprio para a instância |
| Sandbox | Não executa nada | Nenhuma: roda com as permissões do jogador |
| Estabilidade | `preview`, `api = "0.2"` | `experimental` |

Um mod com comportamento e conteúdo costuma ter as duas metades. O Deepbore Quarry é assim: `deepbore_quarry` traz
texturas, receitas e o menu, e `deepbore_quarry_native` traz o código. O WebAssembly saiu do projeto: um `.hmod` com
`mod.wasm` é recusado, mesmo que páginas antigas ainda citem isso.

## Ferramentas

| Ferramenta | Para que serve |
|---|---|
| `cargo horizon-mod` | `new`, `build`, `run` e `check` de mods de conteúdo; `check` também valida `.hnmod` |
| [horizon-mods](https://github.com/horizon-rust/horizon-mods) `native/tools/native.py` | `check`, `build` e `package` de mods nativos |
| [horizon-mixins](https://github.com/horizon-rust/horizon-mixins) | Aplica o bundle no código do Horizon antes de compilar |
| `horizon-launcher native-build` | Compila o jogo com o mod, como acontece com o jogador |

```sh
cargo install --git https://github.com/horizon-rust/horizon cargo-horizon-mod
git clone https://github.com/horizon-rust/horizon-mods
```

Requisitos: Rust estável, Git, Python 3.11 ou mais novo, rsync e, no Linux, os arquivos de desenvolvimento de ALSA e
libudev. Para jogar é preciso ter uma cópia legal do Minecraft.

## Mod de conteúdo

```
ruby/
├── horizon-mod.toml
├── data/ruby/horizon/block/ruby_block.json     bloco: um template vanilla e o que muda
├── data/ruby/horizon/item/ruby.json            item: idem
├── data/ruby/horizon/menu/machine.json         tipo de menu próprio
├── data/ruby/recipe/...                        formato vanilla 26.3
├── data/ruby/loot_table/blocks/ruby_block.json
├── data/ruby/worldgen/configured_feature/...
├── data/ruby/worldgen/placed_feature/...
├── data/ruby/neoforge/biome_modifier/...       injeta features em biomas vanilla
├── data/minecraft/tags/block/mineable/pickaxe.json
├── assets/ruby/blockstates, models, items, textures
└── assets/ruby/lang/en_us.json
```

```toml
id = "ruby"              # a-z, 0-9 e _; é o namespace (ruby:ruby_block)
name = "Ruby"
version = "0.1.0"
description = "Uma gema e um bloco"
authors = ["Você"]
api = "0.2"
side = "both"            # sem esta linha o padrão é client e a checagem de servidor ignora o mod
capabilities = ["registries"]
```

Blocos e itens sempre copiam um template vanilla:

```json
{ "template": "minecraft:iron_ore", "properties": { "hardness": 3.0, "light": 3 } }
```

Propriedades de bloco: `hardness`, `explosion_resistance`, `light`, `requires_tool`, `friction`, `map_color`, `sounds`.
Fora de `properties`: `shapes`, `collision_shapes`, `state_properties`, `default_state`, `connects_to`, `thickness`,
`item`, `components`, entre outras. Chave desconhecida é erro de build; arquivo que o jogo não consegue ler vai para o
log e é pulado.

Um bloco que conecta, como um cabo, usa o template do chorus plant:

```json
{ "template": "minecraft:chorus_plant", "connects_to": "#mymod:cable_connects" }
```

Minério em biomas vanilla usa o biome modifier do NeoForge, que o Horizon lê:

```json
{ "type": "neoforge:add_features", "biomes": "#minecraft:is_overworld",
  "features": "ruby:ore_ruby", "step": "underground_ores" }
```

Um menu próprio declara slots, data slots e slots só de saída:

```json
{ "slots": 9, "data_slots": 2, "results": [7, 8] }
```

Fluxo:

```sh
cargo horizon-mod new ruby --name "Ruby"
cd ruby
cargo horizon-mod build                                # target/horizon-mod/ruby.hmod
cargo horizon-mod check target/horizon-mod/ruby.hmod   # as checagens do registro
cargo horizon-mod run --game <horizon-client>          # ou --dev-mod "/caminho" nos argumentos do jogo
```

Limites: 65.536 arquivos, 16 MiB por arquivo, 512 MiB no total. Só tags podem ir em `data/minecraft/`.

## Mod nativo

O `horizon-mixins` cola o código do mod no código-fonte do Horizon antes da compilação. O jogo oferece mais de 400
**hook points**: funções vazias chamadas em lugares fixos (tick, HUD, bloco colocado, menu). Ao lado deles há cerca de
160 **helpers**. O catálogo é o `native-hooks.json`, na raiz do Horizon, e cada entrada é explicada em
`docs/native-mods.md`.

```
meu_mod/
├── Cargo.toml                 lib apontando para runtime.rs: roda cargo test
├── compatibility.json         versão de hooks exigida e commits testados
└── mixins/meu_mod/
    ├── horizon-mixin.toml     manifesto
    ├── runtime.rs             regras puras, sem tipos do jogo
    ├── glue.rs                tudo que nomeia tipos do jogo
    └── tick.rs, blocks.rs...  um helper curto por hook
```

```toml
[mod]
id = "meu_mod"
version = "0.1.0"
priority = 100

[compatibility]
horizon_revision = "6475f62f3c834b7489e1816bf47f47ea5eb2b879"

[[module]]                     # vira crate::game::meu_mod
id = "rules"
file = "crates/horizon-client/src/game.rs"
name = "meu_mod"
source = "runtime.rs"

[[injection]]
id = "block-entity-tick"
hook = "block_entity_tick"     # nome do catálogo, sobrevive a mudanças de arquivo
hook_version = 1
kind = "HEAD"
helper = "block_entity_tick.rs"
bindings = [
  { name = "game", ty = "&mut crate::game::Game<'_>", value = "&mut *self" },
  { name = "pos", ty = "[i32; 3]", value = "pos" },
  { name = "entity", ty = "&mut horizon_client::containers::ModBlockEntity", value = "&mut *entity" },
]
```

Outras seções do manifesto:

| Seção | Função |
|---|---|
| `[[crate]]` | Pacote Cargo do mod, compilado uma vez. Só usa crates do `Cargo.lock` do Horizon e não pode depender de `horizon-client` |
| `[api]` | Crate da API tipada, descrita abaixo |
| `[dependencies]` | Outros mods tipados que este chama |
| `[[post_build]]` | Binário de um crate do mod que roda depois da compilação do jogo |

Arquivos de `module` e helper não aceitam atributos internos nem comentários `//!`. Um helper tem uma função só, cujos
parâmetros são os `bindings` em ordem.

### Mixin ou API tipada

A API tipada troca os helpers por um crate que implementa `horizon_client::mod_api::Hooks`, disponível desde a Nightly
de 2026-11-03. Todos os hooks do catálogo têm método no trait.

| | Helpers mixin | API tipada (`[api]`) |
|---|---|---|
| Registro de mods | Sim | Ainda não: `native.py` não trata `[api]` e `package` deixa o crate de fora |
| Acesso | Tudo que está em `crate::game` | Só o que o `Host` expõe |
| Energia entre blocos | `native_energy_insert`, `native_move_energy` | Ausente: esses helpers não estão no `Host` |
| Estado | `thread_local!` | Atômicos ou `Mutex`, tipo `Send + Sync` |
| Testes | Regras puras com `cargo test` | `dispatch_<hook>` com um `Host` de teste |

Um mesmo bundle pode usar os dois. Mixins rodam antes dos mods tipados.

### Anatomia de uma máquina

| Hook | Função |
|---|---|
| `blocks`, `items` | Declara blocos e itens em código, como o `DeferredRegister` |
| `block_entity` | Cria a block entity do bloco |
| `block_entity_ticks` | Liga o tick |
| `block_entity_tick` | A lógica por tick, no servidor |
| `block_entity_container` | Slots, faces, `inputs`, `outputs` e menu (até 256 slots) |
| `block_entity_energy` | Buffer de energia, `Capabilities.Energy.BLOCK` |
| `block_entity_fluids` | Tanques, até 16, com 1 a 1.000.000 mB |
| `block_entity_update_tag` | O que vai para o cliente desenhar |
| `block_entity_submit` | Desenho por frame, até 64 blocos da câmera |
| `screen`, `screen_draw`, `screen_click` | Tela do menu, no cliente |
| `menu_button`, `menu_slot_rules`, `menu_quick_move` | Botões, regra de slot e shift-click |

Mover coisas entre blocos: `native_insert_item`, `native_extract_item`, `native_move_items`, `native_fluid_insert`,
`native_move_fluids`, `native_energy_insert`, `native_energy_extract`, `native_move_energy`. Para mexer na block entity
vizinha: `native_with_mod_block_entity`. Receitas com tipo próprio: `native_recipes_of_type`,
`native_recipe_ingredient` e `native_recipe_result`.

Um gerador mínimo, compilado contra o Horizon (ver [O que foi verificado](#o-que-foi-verificado)):

```rust
pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if block == rules::GENERATOR {
        *energy = Some(ModEnergyBuffer {
            capacity: rules::CAPACITY,
            max_insert: 0,
            max_extract: rules::CAPACITY,
            insert_faces: [false; 6],
            extract_faces: [true; 6],
        });
    }
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if entity.id != rules::GENERATOR {
        return;
    }
    let key = JavaString::from("energy");
    let stored = match entity.nbt.get(&key) {
        Some(Tag::Int(stored)) => *stored,
        _ => 0,
    };
    let mut stored = rules::generated(stored);
    for (face, offset) in SIDES {
        if stored == 0 {
            break;
        }
        let neighbour = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
        stored -= game.native_energy_insert(neighbour, face, stored, false);
    }
    entity.nbt.insert(key, Tag::Int(stored));
}
```

O tick lê e grava o próprio NBT porque a block entity que está rodando sai do mundo durante a chamada. Por isso
`native_energy` responde `None` para ela e `native_move_energy` não serve para esvaziar o bloco que está em tick.

## Compartilhar código entre mods nativos

Todos os helpers e módulos entram no mesmo crate, `horizon-client`, então um mod enxerga o que outro adicionou, desde
que os dois estejam no build. Não existe dependência declarada para isso. Resultado de `native.py check` no commit
`6475f62` com `horizon-mixins` `e8cd0aa`:

| Cenário | Resultado |
|---|---|
| Consumidor chama módulo e `[[crate]]` de um mod de lib, os dois no build | OK |
| O mesmo consumidor sem a lib | Falha de compilação, `E0433` |
| `[dependencies]` em bundle só de mixin | `E_MANIFEST`: o `horizon-mixins` recusa o campo |
| Mesmo crate copiado em dois mods, mesmo nome de pacote | `E_CRATE`: nomes de pacote colidem |
| Mesmo crate copiado em dois mods, nomes de pacote diferentes | OK juntos e cada um sozinho |

O `[dependencies]` funcional é o da API tipada, entre pacotes `[api]`, que o launcher resolve com erros claros. Ele
fica do outro lado do grafo: o crate `[api]` depende do jogo, então um helper mixin não consegue chamá-lo. O registro só
tem `requires` para modpacks, então instalar um mod não puxa o que ele usa.

Um bundle pode ter mixin, `[api]` e `[dependencies]` ao mesmo tempo. O launcher tira os dois últimos antes de chamar o
`horizon-mixins` (`api::strip`) e resolve a dependência, com erro claro para mod faltando, versão errada ou ciclo. Toda a
cadeia precisa ter `[api]`, ainda que o crate seja mínimo. Uma cadeia de três mods assim compilou pelo launcher, com o
crate `[api]` de cada um chamando o do mod de baixo. Um `.hnmod` nesse formato passa em `cargo horizon-mod check`, mas
`native.py` não aceita o manifesto e `package` não inclui o crate.

O assunto está em andamento. A API de mods é desenvolvida primeiro no fork
[arismoko/horizon](https://github.com/arismoko/horizon):

| Issue | Tema |
|---|---|
| [horizon#7386](https://github.com/horizon-rust/horizon/issues/7386) | Mods grandes sem mixin: dylibs e "parts", que incluem capabilities |
| [horizon#7387](https://github.com/horizon-rust/horizon/issues/7387) | Métodos de energia, fluido e inventário pausados até as capabilities |
| [arismoko#1005](https://github.com/arismoko/horizon/issues/1005) | Ligar dylibs de addon e mod base |
| [arismoko#1004](https://github.com/arismoko/horizon/issues/1004) | Eventos tipados de um mod base para seus addons |
| [arismoko#1000](https://github.com/arismoko/horizon/issues/1000) | `[dependencies]` em mods de conteúdo, hoje recusado |

Hoje um mod base publica só funções e estáticos. A proposta em #7386 também prevê remover helpers por ID cobertos pelos
novos handles, o que atingiria código mixin ao trocar de pin.

## Testar

```sh
cargo test                                                    # regras puras, sem jogo
python3 native/tools/native.py check /caminho/meu_mod         # aplica o bundle e roda cargo check, -D warnings
python3 native/tools/native.py check /caminho/meu_mod --horizon latest
horizon-launcher settings native-profile dev                  # rebuild cerca de 6x mais rápido
horizon-launcher native-build /caminho/meu_mod/mixins/meu_mod --trust-native-code
```

No jogo: ligar e desligar o mod em Options > Mods, testar comandos, entrar em singleplayer e em servidor, sair e voltar
ao mundo. O primeiro `native-build` baixa o Rust do launcher e leva de 20 a 30 minutos, segundo a documentação. O
Deepbore usa `horizon-ctl` para dirigir o jogo em testes sem tela (`tools/ingame_test.py`).

Em singleplayer, o jogo da janela e o do servidor rodam em threads separadas, então cada um enxerga o seu
`thread_local!`. Um panic no mod derruba o jogo.

## Versões e pin

Cada hook novo aumenta `NATIVE_HOOKS_API`. A Nightly de 2026-11-04 tem 359, e `main` já está em 409, então confira se
o hook existe na Nightly antes de usar.

- **Só hook points:** o mod declara `"hooks": N` e compila em qualquer Horizon com versão N ou maior.
- **Funções internas do jogo:** o mod fica preso ao commit de `horizon_revision`, e todos os mods pinados de um build
  precisam concordar ou o jogador confirma um **Build anyway**.

O pin deve ser o commit de uma Nightly (`nightly-source/<data>-<curto>`), que continua compilável depois de a Nightly
ser substituída. O `horizon_mixins_revision` de referência é `e8cd0aa`, o mesmo do launcher e dos mods oficiais.

## Publicar

1. Código público sob licença aceita: MIT, MIT-0, BSD, 0BSD, Apache-2.0, ISC, zlib, Unlicense ou CC0-1.0. GPL, LGPL
   e MPL não passam sem autorização escrita para os revisores lerem o código.
2. `cargo horizon-mod build` ou `native.py package`, depois `cargo horizon-mod check <arquivo>`.
3. Assinatura opcional com minisign. Depois da primeira, toda versão precisa dela.
4. Envio em [playhorizon.dev/mods/submit](https://playhorizon.dev/mods/submit), que abre um PR em `horizon-mods`.
5. Uma pessoa revisa. Todo o código nativo é lido de novo a cada versão.

Um port precisa de licença que permita, mantém os avisos de copyright, credita os autores e aponta o original. Não
pode trazer código ou assets da Mojang. Um `.hnmod` aceita até 512 arquivos e 64 MiB descompactados, só `.rs`, `.toml`,
pastas `data/` e arquivos de texto como `README` e `LICENSE`.

## Armadilhas

- Faces de `ModContainer`, `ModFluidTanks` e `ModEnergyBuffer` são pedidas uma vez por tipo de bloco, não por
  instância, e são direções absolutas.
- Data slots de menu viajam em 16 bits. Valor maior precisa ser dividido em dois slots ou ir por outro canal.
- Energia é `i32`, gravada no NBT como o int `energy`.
- Fluidos de mod existem em tanques, mas ainda não como blocos no mundo nem em baldes.
- O tick de block entity só roda no servidor. A tela lê data slots, o update tag ou `native_screen_extra_data`.
- `native_menu_slot_rules` roda no crate da biblioteca, sem `Game`, e não alcança os módulos do mod.
- `native.py check` usa `-D warnings`: código usado só em testes precisa de `#[cfg(test)]`.
- Dedicated server exige export próprio (`--server-output`), e mods nativos ainda não entram na checagem de mods ao
  conectar.
- Mods de conteúdo são lidos só ao iniciar o jogo: depois de editar, reinicie.
- `native-build` recusa versão não preparada antes de resolver dependências ou compilar. O `prepare` precisa de uma
  instalação do Minecraft 26.3 ou de uma conta Microsoft que tenha o jogo.

## O que foi verificado

Executado nesta máquina em 2026-11-05 (Rust 1.99.0, Linux x86_64):

| Item | Resultado |
|---|---|
| Compilar `cargo-horizon-mod` em `main` | OK, 1m14s |
| `new`, `build` e `check` do template de conteúdo | OK, `.hmod` de 2.442 bytes |
| `new --template native --horizon <commit>` | OK |
| Mod com bloco, item, `configured_feature`, `placed_feature`, biome modifier e receita `mekanism:enriching` | `build` e `check` OK |
| Chave desconhecida em bloco | `build` falha com a lista de chaves válidas |
| `cargo test` das regras do gerador | OK |
| `native.py check` do gerador acima, no commit `6475f62` (API 359) | OK, 3m14s na primeira vez, incluindo baixar o Horizon e compilar o `horizon-mixins` |
| O mesmo check com um método inexistente no `glue.rs` | Falha com `E0599`, em 30s |
| `native.py package` e `cargo horizon-mod check` do `.hnmod` | OK, 3.752 bytes, 9 arquivos |
| `.hnmod` com mixin, `[api]` e `[dependencies]`, montado à mão | `cargo horizon-mod check` OK |
| `native-build` de três mods híbridos em cadeia, perfil `dev`, Launcher 0.2.42, no desktop | OK, 6m23s; binário de 167 MB com os 3 mods, `--launch-api` responde |
| Gerador e bateria de teste num mundo, no jogo em container | A bateria sobe 100 por tick, e o gerador fica em 0 |
| Fechar e reabrir o mundo com a bateria | Energia preservada: o NBT `energy` é salvo |
| A mesma cadeia sem a lib | `mekanism requires missing mod machinery_basis` |
| A mesma cadeia com requisito `^2` | `mekanism requires machinery_basis ^2, but installed version is 0.1.0` |

Não executado: worldgen de mod, telas de máquina e carregamento de receitas próprias vêm só da leitura de docs e código.
`build`, `check` e `native-build` validam formato e compilação, não comportamento.

## Fontes

- Guias: [make-a-mod](https://playhorizon.dev/docs/make-a-mod), [mod-manifest](https://playhorizon.dev/docs/mod-manifest),
  [mod-code](https://playhorizon.dev/docs/mod-code), [content-mods](https://playhorizon.dev/docs/content-mods),
  [test-a-mod](https://playhorizon.dev/docs/test-a-mod), [publish-a-mod](https://playhorizon.dev/docs/publish-a-mod),
  [estabilidade da API](https://playhorizon.dev/docs/api/stability)
- No repositório do Horizon: `docs/mods.md`, `docs/native-mods.md`, `native-hooks.json`,
  `crates/horizon-client/src/game/native_hooks.rs`
- [Política de mods](https://github.com/horizon-rust/horizon-mods/blob/main/MOD-POLICY.md)
- Exemplos reais: [Deepbore Quarry](https://github.com/Bioneisme/deepbore-quarry) (máquina pequena, a melhor referência),
  [Create for Horizon](https://github.com/Delete228/create-horizon) (port grande, cerca de 34 mil linhas de Rust)
