#!/usr/bin/env python3
"""Gera a tabela dos displays do livro de receitas do Mekanism em `mods/mekanism/native/runtime.rs` (o módulo `recipe_book_rows`).

Uso: livro_receitas_tabela.py [--check]

Lê `rastros/recipe_book_displays.jsonl` (o que `testes/RecipeBookDisplaysTraceTest.java` mede no original) e escreve, entre os marcadores
`// recipe-book-rows:inicio` e `// recipe-book-rows:fim`, uma linha por display das quatro máquinas: o id (`RecipeDisplayId`, o índice na
lista de todos os displays do jogo, que o Horizon não numera porque o `RecipeIndex` lê só `data/minecraft/recipe`), a receita, o tipo, o
grupo (`bookInfo.group()`, também numerado em todas as receitas do jogo), a categoria do livro, o item do resultado e as linhas do
tooltip dele. De `rastros/recipe_book_requirements.jsonl` (`testes/RecipeBookStateTraceTest.java`) sai a tabela `REQUIREMENTS`: os
`craftingRequirements` de cada display (os itens que cada `Ingredient` aceita) e a quantidade que cada ingrediente do
`MekanismRecipeDisplay` pede, que o `canCraft` do livro usa. Com `--check` só confere se o arquivo já está igual ao que o gerador
escreveria. A tabela `GHOSTS` (o que o fantasma de cada display desenha) sai dos itens que cada ingrediente e cada saída mostram
(`ing` e `out` do mesmo rastro) e dos químicos deles (`chem` e `chem_out` de `rastros/recipe_place_info.jsonl`, de
`testes/RecipePlaceTraceTest.java`). A tabela `DISPLAYS` (todas as receitas do jogo com display, com o primeiro id e quantos) sai de
`rastros/recipe_book_known_displays.jsonl` (`testes/RecipeBookKnownTraceTest.java`).
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
TRACE = os.path.join(HERE, "rastros", "recipe_book_displays.jsonl")
REQUIREMENTS = os.path.join(HERE, "rastros", "recipe_book_requirements.jsonl")
PLACE_INFO = os.path.join(HERE, "rastros", "recipe_place_info.jsonl")
KNOWN = os.path.join(HERE, "rastros", "recipe_book_known_displays.jsonl")
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


def requirements():
    out = []
    with open(REQUIREMENTS) as handle:
        for line in handle:
            out.append(json.loads(line))
    out.sort(key=lambda row: row["id"])
    return out


def place_info():
    out = {}
    with open(PLACE_INFO) as handle:
        for line in handle:
            row = json.loads(line)
            out[row["id"]] = row
    return out


def known_displays():
    """Cada receita do jogo com display, na ordem de `getRecipes()`: o nome, o primeiro id e quantos (os ids seguem a ordem, do zero)."""
    out = []
    first = 0
    with open(KNOWN) as handle:
        for line in handle:
            row = json.loads(line)
            if "header" in row:
                assert row["ascending"] is True
                total = row["displays"]
                continue
            if row["n"]:
                out.append((row["recipe"], first, row["n"]))
            first += row["n"]
    assert first == total, (first, total)
    return out


def shown(lists):
    return "&[%s]" % ", ".join("&[%s]" % ", ".join("(%s, %d)" % (rust_str(name), count) for name, count in one) for one in lists)


def rust_str(text):
    return json.dumps(text, ensure_ascii=False)


def render():
    lines = [BEGIN, "#[rustfmt::skip]", "pub mod recipe_book_rows {"]
    lines.append(
        "    //! GERADO por `dev/referencia/livro_receitas_tabela.py` a partir de `rastros/recipe_book_displays.jsonl`: não edite à mão."
    )
    lines.append(
        "    /// Um display que o original manda ao livro: `(id, receita, tipo, grupo (-1 sem grupo), categoria, item do resultado, quantidade, tooltip, requisitos)`, com `requisitos` o tamanho de `craftingRequirements` (-1 sem a lista)."
    )
    lines.append(
        "    pub type Row = (i32, &'static str, &'static str, i32, &'static str, &'static str, i32, &'static [&'static str], i32);"
    )
    lines.append("    pub const ROWS: &[Row] = &[")
    for row in rows():
        group = -1 if row["group"] is None else row["group"]
        result = row["results"][0]
        tooltip = ", ".join(rust_str(text) for text in row["tooltip"])
        lines.append(
            "        (%d, %s, %s, %d, %s, %s, %d, &[%s], %d),"
            % (
                row["id"],
                rust_str(row["recipe"]),
                rust_str(row["type"]),
                group,
                rust_str(row["category"]),
                rust_str(result["item"]),
                result["count"],
                tooltip,
                -1 if row["requirements"] is None else row["requirements"],
            )
        )
    lines.append("    ];")
    lines.append(
        "    /// O que cada display pede: `(id, craftingRequirements (os itens de cada ingrediente), quantidades)`, com `quantidades` o `resolveForFirstStack(...).count()` de cada ingrediente de um `MekanismRecipeDisplay` (`None` nos outros displays)."
    )
    lines.append(
        "    pub const REQUIREMENTS: &[machinery_basis_core::recipe_book_view::Requirement] = &["
    )
    for row in requirements():
        if row["req"] is None:
            req = "None"
        else:
            req = "Some(&[%s])" % ", ".join(
                "&[%s]" % ", ".join(rust_str(item) for item in items) for items in row["req"]
            )
        counts = "None" if row["counts"] is None else "Some(&[%s])" % ", ".join(str(n) for n in row["counts"])
        lines.append("        (%d, %s, %s)," % (row["id"], req, counts))
    lines.append("    ];")
    lines.append(
        "    /// O que o fantasma de cada display desenha: `(id, itens de cada ingrediente, itens de cada saída, químicos de cada ingrediente, químicos de cada saída)`."
    )
    lines.append("    pub const GHOSTS: &[machinery_basis_core::recipe_book_view::GhostRow] = &[")
    info = place_info()
    for row in requirements():
        extra = info[row["id"]]
        lines.append(
            "        (%d, %s, %s, %s, %s),"
            % (row["id"], shown(row["ing"]), shown(row["out"]), shown(extra["chem"]), shown(extra["chem_out"]))
        )
    lines.append("    ];")
    lines.append(
        "    /// Todas as receitas do jogo que têm display, na ordem do `RecipeManager` do original (`rastros/recipe_book_known_displays.jsonl`): `(receita, id do primeiro display, quantos)`. O `ClientRecipeBook` guarda os displays de todas as que o jogador conhece, e a tabela dele decide a ordem das coleções do livro."
    )
    lines.append("    pub const DISPLAYS: &[machinery_basis_core::recipe_book_data::DisplayRow] = &[")
    for name, first, count in known_displays():
        lines.append("        (%s, %d, %d)," % (rust_str(name), first, count))
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
