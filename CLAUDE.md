# rust-mekanism

## Onde o código fica

Uma mecânica vai para a lib `machinery_basis` quando outros mods puderem reaproveitá-la ou quando vários blocos diferentes
do Mekanism a usarem: o que se repete dentro do mod provavelmente serve a outros mods. No mod fica só o que é daquele bloco:
valores, receitas, texturas e o layout da tela.

O critério completo está em [docs/machinery-basis.md](docs/machinery-basis.md#regra-de-escopo). A lib continua genérica: ela
não conhece nenhum id, químico ou máquina do Mekanism, que entram como dados ou parâmetros.
