# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- Base de máquina da `machinery_basis` portada do Mekanism como objetos vivos: transações
  com diários que se registram sozinhos, contêineres compartilhados, `CachedRecipe` com closures, monitor de receita, tile e
  o ejetor de itens numa transação que cobre a máquina e o baú, conferidos tick a tick com a Enrichment Chamber original.
- Enrichment Chamber ligada à base nova no jogo (as outras máquinas seguem no código antigo), com `MEKTRACE` e `MEKBENCH`
  atrás de `MEDIR`. Ejetor de itens (`/data merge block x y z {mekanism_eject:1}`, saída à direita), redstone, comparador e o
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
