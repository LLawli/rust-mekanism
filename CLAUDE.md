# rust-mekanism

## Objetivo: paridade 1:1

Este projeto é um port do Mekanism e do Mekanism Generators: o comportamento tem de ser idêntico ao do mod original, branch
26.3, em lógica, valores, visual, som, telas e interação.

- Nada se decide por simplicidade ou desempenho. Antes de escrever, leia o Java e reproduza o que ele faz, inclusive as
  otimizações, que são parte do comportamento.
- Não existe versão simplificada nem diferença aceitável por conveniência. Medir serve para confirmar paridade, não para
  escolher outro caminho.
- Se o Horizon não permitir algo, tente primeiro uma injeção no jogo. Se nem isso der, pare e pergunte ao usuário. Nunca
  troque por uma aproximação sem autorização, e nunca diga que algo é igual ao original sem ter comparado com o código dele.
- O plano e o inventário do que ainda diverge estão em [docs/plano-paridade.md](docs/plano-paridade.md).

## Testes

O máximo possível roda sem entrar no jogo e sem ninguém interagir. Prefira um teste que emula o jogo tick a tick e compara os
estados, com as regras em código puro e um mundo simulado por trás. A referência dos valores é o Java do original. Teste
manual em mundo fica para o que só o jogo mostra, como visual e som.

## Onde o código fica

Uma mecânica vai para a lib `machinery_basis` quando outros mods puderem reaproveitá-la ou quando vários blocos diferentes
do Mekanism a usarem: o que se repete dentro do mod provavelmente serve a outros mods. No mod fica só o que é daquele bloco:
valores, receitas, texturas e o layout da tela.

O critério completo está em [docs/machinery-basis.md](docs/machinery-basis.md#regra-de-escopo). A lib continua genérica: ela
não conhece nenhum id, químico ou máquina do Mekanism, que entram como dados ou parâmetros.
