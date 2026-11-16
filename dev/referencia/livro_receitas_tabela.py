#!/usr/bin/env python3
"""Gera a tabela dos displays do livro de receitas do Mekanism em `mods/mekanism/native/runtime.rs` (o módulo `recipe_book_rows`).

Uso: livro_receitas_tabela.py [--check]

Lê `rastros/recipe_book_displays.jsonl` (o que `testes/RecipeBookDisplaysTraceTest.java` mede no original) e escreve, entre os marcadores
`// recipe-book-rows:inicio` e `// recipe-book-rows:fim`, uma linha por display das quatro máquinas: o id (`RecipeDisplayId`, o índice na
lista de todos os displays do jogo, que o Horizon não numera porque o `RecipeIndex` lê só `data/minecraft/recipe`), a receita, o tipo, o
grupo (`bookInfo.group()`, também numerado em todas as receitas do jogo), a categoria do livro, o item do resultado e as linhas do
tooltip dele. Com `--check` só confere se o arquivo já está igual ao que o gerador escreveria.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
TRACE = os.path.join(HERE, "rastros", "recipe_book_displays.jsonl")
RUNTIME = os.path.join(ROOT, "mods", "mekanism", "native", "runtime.rs")
BEGIN = "// recipe-book-rows:inicio"
END = "// recipe-book-rows:fim"


def rows():
    out = []
    with open(TRACE) as handle:
        for line in handle:
            row = json.loads(line)
            if "id" in row:
                out.append(row)
    out.sort(key=lambda row: row["id"])
    return out


def rust_str(text):
    return json.dumps(text, ensure_ascii=False)


def render():
    lines = [BEGIN, "#[rustfmt::skip]", "pub mod recipe_book_rows {"]
    lines.append(
        "    //! GERADO por `dev/referencia/livro_receitas_tabela.py` a partir de `rastros/recipe_book_displays.jsonl`: não edite à mão."
    )
    lines.append(
        "    /// Um display que o original manda ao livro: `(id, receita, tipo, grupo (-1 sem grupo), categoria, item do resultado, quantidade, tooltip)`."
    )
    lines.append(
        "    pub type Row = (i32, &'static str, &'static str, i32, &'static str, &'static str, i32, &'static [&'static str]);"
    )
    lines.append("    pub const ROWS: &[Row] = &[")
    for row in rows():
        group = -1 if row["group"] is None else row["group"]
        result = row["results"][0]
        tooltip = ", ".join(rust_str(text) for text in row["tooltip"])
        lines.append(
            "        (%d, %s, %s, %d, %s, %s, %d, &[%s]),"
            % (
                row["id"],
                rust_str(row["recipe"]),
                rust_str(row["type"]),
                group,
                rust_str(row["category"]),
                rust_str(result["item"]),
                result["count"],
                tooltip,
            )
        )
    lines.append("    ];")
    lines.append("}")
    lines.append(END)
    return "\n".join(lines) + "\n"


def main():
    text = open(RUNTIME).read()
    block = render()
    if BEGIN in text:
        start = text.index(BEGIN)
        stop = text.index(END) + len(END) + 1
        new = text[:start] + block + text[stop:]
    else:
        marker = "#[cfg(test)]\nmod tests {"
        at = text.rindex(marker)
        new = text[:at] + block + "\n" + text[at:]
    if "--check" in sys.argv:
        if new != text:
            print("a tabela está velha: rode livro_receitas_tabela.py", file=sys.stderr)
            sys.exit(1)
        print("ok")
        return
    open(RUNTIME, "w").write(new)
    print("escrito", len(rows()), "linhas")


main()
