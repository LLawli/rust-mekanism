# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- Upgrades das máquinas do Mekanism: o item `mekanism:upgrade` com o componente `mekanism:upgrade_type`, os tipos de velocidade,
  energia, abafamento e contenção de experiência (os que a tag `default_machine` lista) com os máximos e as tags originais, as receitas
  de velocidade, energia e contenção de experiência, as texturas e os nomes. Shift e clique direito com o upgrade na mão instala nas
  quatro máquinas, como o `ItemUpgrade.useOn`. Tempo, consumo e capacidade seguem o que está instalado, também depois de reabrir o
  mundo. Os slots de entrada e de saída e a instalação em 20 ticks existem e estão testados, mas ainda não aparecem na tela.
- `upgrade.rs` na `machinery_basis`: o `TileComponentUpgrade` (`tickServer`, `addUpgrades`, `removeUpgrade`, `serialize` e
  `deserialize`), os dois `UpgradeInventorySlot` e os tipos e tags de upgrade lidos de dados. Conferido tick a tick com o rastro
  `chamber_upgrade_slot` do original, e com um teste de salvar e reabrir com upgrades instalados e nos slots.
- A capacidade de energia de uma máquina é a da instância, e não a do bloco: uma injeção em `native_energy_at` responde com a que os
  upgrades dão. A tela passa a ler a capacidade dos data slots.
- As máquinas do Mekanism dão sinal a comparadores, pelo quanto os slots estão cheios.
- Metallurgic Infuser na base de máquina: tanque de químico com diário e listener (`ChemicalTank`), `TwoInputCachedRecipe`
  de item e químico, `fillTankOrConvert` e a `ProgressMachine`, que as máquinas elétricas já usavam e agora serve também a
  máquina com tanque. Conferido tick a tick com o rastro do Infuser original (`infuser_run`), com teste de salvar e reabrir.
  O `MEKTRACE` ganha `tank` e `infusion`. Fora de escopo: `ItemStackConstantChemicalToObjectCachedRecipe` e o `usedSoFar`
  que ela muda, porque as 19 receitas de infusão têm `per_tick_usage: false`. A tag `used_so_far` é lida e gravada como no Java.
- O ejetor respeita a regra de cada slot do vizinho: pela lateral, um forno só recebe combustível.
- Base de máquina da `machinery_basis` portada do Mekanism como objetos vivos: transações
  com diários que se registram sozinhos, contêineres compartilhados, `CachedRecipe` com closures, monitor de receita, tile e
  o ejetor de itens numa transação que cobre a máquina e o baú, conferidos tick a tick com a Enrichment Chamber original.
- Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser ligados à base nova no jogo, com `MEKTRACE` e
  `MEKBENCH` atrás de `MEDIR`. Ejetor de itens (`/data merge block x y z {mekanism_eject:1}`, saída à direita), redstone, comparador e o
  NBT que o Java persiste (redstone, modo, estado ativo, atraso, nível do comparador, progresso, receitas usadas).
- Regras de hopper da `machinery_basis`: a máquina entrega ao hopper abaixo e recebe do hopper que aponta para ela,
  respeitando as faces configuradas, a 1 item por 8 ticks.
- Recursos tipados declarados por dados e tanques da `machinery_basis`.
- Bundle da `machinery_basis`: tanques salvos no NBT, registro de recursos, distribuição de energia, regra de hoppers e
  ejetor de itens.
- Heat Generator jogável, com tela, e minério de ósmio gerado no mundo.
- Metallurgic Infuser jogável, com tela, as receitas de infusão originais e os químicos de infusão, e os itens do aço, do
  circuito básico e da liga infundida.
- Steel Casing, Enrichment Chamber, Crusher e Energized Smelter jogáveis, com as receitas de enriquecimento e trituração
  originais, os pós, os itens enriquecidos e os circuitos e ligas que eles destravam.
- Universal Cable nos quatro tiers: leva energia dos geradores às máquinas, dividindo por igual entre quem consome.
- Rede de energia na `machinery_basis`, com a travessia das ligações e a divisão justa.
- Máquinas de progresso na `machinery_basis`: receitas de item e recurso, conversão de item em recurso, tags de recursos e
  hoppers que alcançam slots diferentes por face.
- Lava dentro do bloco na `machinery_basis`: um bloco com a propriedade `fluid_logged` recebe lava com shift + balde, e ela
  corre como lava real, ilumina e fica no lugar quando o bloco quebra.

### Changed

- O tanque do Metallurgic Infuser deixa o formato `machinery_basis.tanks` e passa ao do Java: `chemical_tanks`, uma lista de
  `{container, stored: {type, amount}}`, sem a lista quando está vazio. O que já estava salvo no formato anterior é levado ao
  novo no primeiro tick do bloco. O Heat Generator segue no formato antigo.
- O Infuser ganha o ejetor de itens (`mekanism_eject`), o controle de redstone, o comparador e o atraso para apagar da base
  nova, no lugar do progresso próprio de antes. `dump()` esvazia o tanque como `dumpOrClearContents`.

- O `ItemResource` da lib ganha `variant`, o que distingue itens de mesmo id (os componentes), e o `InventorySlot` ganha o
  `validator`, o item no `canInsert` e `Access::ManualOnly`.

### Removed

- `ProgressMachine::set_upgrades`, que punha os totais direto: os upgrades entram pelo componente.
- `machine::step` e `run_machine`, que só o Infuser ainda usava, e a conversão de item em tanque do código antigo.
