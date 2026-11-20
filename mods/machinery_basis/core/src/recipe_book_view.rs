//! O que a janela do livro mostra e como ela responde (`VirtualRecipeBookComponent`, `RecipeBookPage`, `MekanismRecipeButton`), sem
//! desenho: os dados de cada máquina (as abas e as linhas dos displays), o livro do cliente montado com o que o jogador desbloqueou,
//! a aba, a busca, o filtro, a página e o botão que cada item da grade mostra. Quem desenha e liga ao mouse e ao teclado é a cola
//! da `machinery_basis`. A lib não conhece nenhum id: as abas e as linhas chegam como dados.

use std::collections::HashMap;

use crate::recipe_book::{
    ClientBook, Craftable, Entry, Page, SEARCH_MAX_LENGTH, display_stack_index, slot_select_index,
};

/// Uma linha da tabela dos displays: `(id, receita, tipo, grupo (-1 sem grupo), categoria, item do resultado, quantidade, tooltip,
/// requisitos)`. `requisitos` é o tamanho de `craftingRequirements` (-1 sem a lista): zero quando só há químico ou fluido nas entradas.
pub type Row = (
    i32,
    &'static str,
    &'static str,
    i32,
    &'static str,
    &'static str,
    i32,
    &'static [&'static str],
    i32,
);

/// Uma aba do livro (`TabInfo`): a categoria que ela lista e os dois itens do ícone (o segundo some quando vazio).
#[derive(Clone, Copy, Debug)]
pub struct BookTab {
    pub category: &'static str,
    pub primary: &'static str,
    pub secondary: Option<&'static str>,
}

/// O que o mod diz do livro de uma máquina: o tipo de livro (compartilhado entre as máquinas do mesmo tipo de receita), as abas
/// depois da bússola, e as linhas de todos os displays (a lista filtra pelo tipo).
#[derive(Debug)]
pub struct BookData {
    pub book_type: &'static str,
    pub tabs: &'static [BookTab],
    pub rows: &'static [Row],
}

/// O ícone da primeira aba, a de busca (`new ItemStack(Items.COMPASS)`).
pub const SEARCH_ICON: &str = "minecraft:compass";

/// A aba escolhida: a de busca (todas as categorias) ou uma das do tipo, por índice em [`BookData::tabs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Search,
    Category(usize),
}

/// Um botão da grade: o que ele desenha agora.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    /// `hasCraftable` da coleção (o fundo `slot_craftable`).
    pub craftable: bool,
    /// Mais de uma receita escolhida (o fundo `slot_many_*`).
    pub many: bool,
    /// O item do resultado da receita que o `SlotSelectTime` escolheu e a quantidade dele.
    pub item: &'static str,
    pub count: i32,
    /// `allRecipesHaveSameResultDisplay` com `many`: o item desenhado duas vezes, uma deslocada.
    pub stacked: bool,
    /// As linhas do tooltip do item mostrado.
    pub tooltip: &'static [&'static str],
}

/// O estado da janela do livro de uma máquina.
#[derive(Debug)]
pub struct View {
    data: &'static BookData,
    book: ClientBook,
    rows: HashMap<i32, &'static Row>,
    pub tab: Tab,
    pub search: String,
    pub filtering: bool,
    /// A caixa de busca tem o foco (o clique nela ou na lupa dá; o clique fora tira).
    pub focused: bool,
    /// `ignoreTextInput`: a tecla do chat focou a caixa e o caractere dela não entra.
    pub ignore_text: bool,
    /// `lastSearch`: o texto da última busca, em minúsculas.
    last_search: String,
    pub page: Page,
    /// `SlotSelectTime.time`: soma o quadro a cada desenho sem o Ctrl.
    pub time: f32,
}

/// `getSearchCategory` do livro: o nome que a aba de busca tem em [`ClientBook`].
fn search_category(data: &BookData) -> String {
    format!("{}:search", data.book_type)
}

impl View {
    /// O livro de quem conhece `unlocked` (os ids dos displays), com `can_craft` dizendo, por linha, se dá para fazê-la
    /// (`MekanismRecipeBookComponent.canCraft`: sem ingrediente de item a receita vale como fazível).
    pub fn new(
        data: &'static BookData,
        unlocked: impl Iterator<Item = i32>,
        filtering: bool,
        can_craft: &dyn Fn(&Row) -> bool,
    ) -> Self {
        let rows: HashMap<i32, &'static Row> = data
            .rows
            .iter()
            .filter(|row| row.2 == data.book_type)
            .map(|row| (row.0, row))
            .collect();
        let mut book = ClientBook::default();
        for id in unlocked {
            if let Some(row) = rows.get(&id) {
                book.add(Entry {
                    id: row.0,
                    category: row.4.to_owned(),
                    group: (row.3 >= 0).then_some(row.3),
                });
            }
        }
        let search = (
            search_category(data),
            data.tabs.iter().map(|tab| tab.category.to_owned()).collect(),
        );
        book.rebuild_collections(&[search]);
        for index in 0..book.collection_count() {
            let collection = book.collection_mut(index);
            let found: Vec<&Row> = collection.entries.iter().map(|entry| rows[&entry.id]).collect();
            collection.names = found
                .iter()
                .flat_map(|row| row.7.iter().map(|line| line.to_lowercase()))
                .collect();
            collection.ids = found
                .iter()
                .filter_map(|row| row.5.split_once(':'))
                .map(|(namespace, path)| (namespace.to_lowercase(), path.to_lowercase()))
                .collect();
            collection.select(|_| true, |entry| can_craft(rows[&entry.id]));
        }
        let mut view = Self {
            data,
            book,
            rows,
            tab: Tab::Search,
            search: String::new(),
            filtering,
            focused: false,
            ignore_text: false,
            last_search: String::new(),
            page: Page::default(),
            time: 0.0,
        };
        view.refresh(true);
        view
    }

    pub fn data(&self) -> &'static BookData {
        self.data
    }

    /// A categoria que a aba escolhida lista no [`ClientBook`].
    fn category(&self) -> String {
        match self.tab {
            Tab::Search => search_category(self.data),
            Tab::Category(index) => self.data.tabs[index].category.to_owned(),
        }
    }

    /// `updateCollections(resetPage)`: refaz a lista da página com a aba, a busca e o filtro de agora.
    pub fn refresh(&mut self, reset_page: bool) {
        let list = self
            .book
            .visible_collections(&self.category(), &self.search, self.filtering);
        self.page
            .update_collections(list, reset_page, self.filtering);
    }

    /// As abas que aparecem, na ordem: a bússola e as categorias que têm receita escolhida (`updateVisibility`).
    pub fn visible_tabs(&self) -> Vec<Tab> {
        std::iter::once(Tab::Search)
            .chain(
                self.data
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| self.book.tab_visible(tab.category))
                    .map(|(index, _)| Tab::Category(index)),
            )
            .collect()
    }

    /// Os dois itens do ícone de uma aba.
    pub fn icons(&self, tab: Tab) -> (&'static str, Option<&'static str>) {
        match tab {
            Tab::Search => (SEARCH_ICON, None),
            Tab::Category(index) => (self.data.tabs[index].primary, self.data.tabs[index].secondary),
        }
    }

    /// `RecipeBookComponent.onTabButtonPress`: só a aba que não é a escolhida muda, e a página volta ao começo (a busca se mantém).
    /// Devolve se mudou; o clique na escolhida só toca o som do botão.
    pub fn select_tab(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        self.tab = tab;
        self.refresh(true);
        true
    }

    /// O botão do filtro (`toggleFiltering`), que a lista de dados guarda por tipo de livro; devolve o novo valor.
    pub fn toggle_filtering(&mut self) -> bool {
        self.filtering = !self.filtering;
        self.refresh(false);
        self.filtering
    }

    /// `EditBox.charTyped` com o cursor no fim: só um caractere que o chat aceita (`StringUtil.isAllowedChatCharacter`: nem `§`, nem
    /// controle, nem DEL) e só até `SEARCH_MAX_LENGTH` (`insertText` corta o que passa). O caractere aceito é consumido mesmo sem caber.
    /// A lista se refaz sem voltar à primeira página (`checkSearchStringUpdate` chama `updateCollections(false, ...)`), e só se o texto
    /// em minúsculas mudou (`lastSearch`).
    pub fn type_char(&mut self, ch: char) -> bool {
        if ch == '\u{a7}' || (ch as u32) < 0x20 || ch == '\u{7f}' {
            return false;
        }
        if self.search.chars().count() < SEARCH_MAX_LENGTH {
            self.search.push(ch);
            self.search_changed();
        }
        true
    }

    /// Backspace com o cursor no fim (`EditBox.deleteText(-1)`): apaga o último caractere.
    pub fn backspace(&mut self) -> bool {
        let changed = self.search.pop().is_some();
        if changed {
            self.search_changed();
        }
        changed
    }

    /// `checkSearchStringUpdate`: refaz a lista, sem voltar à primeira página, quando o texto em minúsculas difere do último.
    fn search_changed(&mut self) {
        let lower = self.search.to_lowercase();
        if lower != self.last_search {
            self.last_search = lower;
            self.refresh(false);
        }
    }

    /// Os botões da página de agora (`updateButtonsForPage` e `MekanismRecipeButton.init`), com o `SlotSelectTime` em `self.time`.
    pub fn buttons(&self) -> Vec<Button> {
        let select = slot_select_index(self.time);
        let status = if self.filtering {
            Craftable::Craftable
        } else {
            Craftable::Any
        };
        self.page
            .shown()
            .iter()
            .filter_map(|&index| {
                let collection = self.book.collection(index);
                let chosen = collection.selected_recipes(status);
                if chosen.is_empty() {
                    return None;
                }
                let (entry, _) = display_stack_index(select, chosen.len());
                let row = self.rows[&chosen[entry].id];
                let same = chosen
                    .iter()
                    .all(|other| self.rows[&other.id].5 == self.rows[&chosen[0].id].5);
                Some(Button {
                    craftable: collection.has_craftable(),
                    many: chosen.len() > 1,
                    item: row.5,
                    count: row.6,
                    stacked: chosen.len() > 1 && same,
                    tooltip: row.7,
                })
            })
            .collect()
    }

    /// O `Page n/m`, só com mais de uma página.
    pub fn page_label(&self) -> Option<(usize, usize)> {
        self.page.page_label()
    }

    /// Quantas coleções a aba de busca guarda com o que o jogador conhece (para os testes).
    pub fn known_collections(&self) -> usize {
        self.book.collection_count()
    }
}
