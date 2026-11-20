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
    /// `RecipeButton.animationTime` de cada um dos 20 botões: 15 quando o botão mostra uma receita destacada (`init`), e desce com o quadro
    /// enquanto o botão é desenhado.
    pub button_anim: [f32; crate::recipe_book::ITEMS_PER_PAGE],
    /// `RecipeBookTabButton.animationTime` de cada aba de categoria (índice em [`BookData::tabs`]): 15 quando a janela abre com uma
    /// receita destacada na aba (`startAnimation`).
    pub tab_anim: Vec<f32>,
    /// As receitas que um botão mostrou e deixaram de ser destacadas (`recipeShown`, `removeRecipeHighlight`), para quem guarda o
    /// conjunto do jogador.
    pub seen: Vec<i32>,
}

/// `ANIMATION_TIME` do botão e da aba.
pub const ANIMATION_TIME: f32 = 15.0;

/// O aperto da animação (`1 + 0.1 * sin(animationTime / 15 * PI)`), em `float` como o Java.
pub fn squeeze(animation_time: f32) -> f32 {
    1.0 + 0.1 * (animation_time / ANIMATION_TIME * std::f32::consts::PI).sin()
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
        highlighted: impl Iterator<Item = i32>,
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
            data.tabs
                .iter()
                .map(|tab| tab.category.to_owned())
                .collect(),
        );
        for id in highlighted {
            book.add_highlight(id);
        }
        book.rebuild_collections(&[search]);
        for index in 0..book.collection_count() {
            let collection = book.collection_mut(index);
            let found: Vec<&Row> = collection
                .entries
                .iter()
                .map(|entry| rows[&entry.id])
                .collect();
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
            button_anim: [0.0; crate::recipe_book::ITEMS_PER_PAGE],
            tab_anim: vec![0.0; data.tabs.len()],
            seen: Vec::new(),
        };
        // `initVisuals`: `updateTabs` (a animação das abas, antes de os botões tirarem o destaque) e depois `updateCollections`.
        for (index, tab) in data.tabs.iter().enumerate() {
            if view.book.tab_visible(tab.category)
                && view.book.tab_highlighted(tab.category, filtering)
            {
                view.tab_anim[index] = ANIMATION_TIME;
            }
        }
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
        self.init_buttons();
    }

    /// `RecipeBookPage.updateButtonsForPage` e `RecipeButton.init`: o botão que mostra uma receita destacada tira o destaque dela
    /// (`recipeShown`) e começa a animação.
    fn init_buttons(&mut self) {
        let status = if self.filtering {
            Craftable::Craftable
        } else {
            Craftable::Any
        };
        let shown: Vec<usize> = self.page.shown().to_vec();
        for (slot, index) in shown.into_iter().enumerate() {
            let newly: Vec<i32> = self
                .book
                .collection(index)
                .selected_recipes(status)
                .iter()
                .map(|entry| entry.id)
                .filter(|&id| self.book.will_highlight(id))
                .collect();
            if !newly.is_empty() {
                for id in newly {
                    self.book.remove_highlight(id);
                    self.seen.push(id);
                }
                self.button_anim[slot] = ANIMATION_TIME;
            }
        }
    }

    /// As setas (`forwardButton` e `backButton`): a página muda e os botões se refazem.
    pub fn forward(&mut self) -> bool {
        let moved = self.page.forward();
        if moved {
            self.init_buttons();
        }
        moved
    }

    pub fn back(&mut self) -> bool {
        let moved = self.page.back();
        if moved {
            self.init_buttons();
        }
        moved
    }

    /// O quadro desenhado (`a`, em ticks): a animação de cada botão visível e de cada aba visível desce.
    pub fn advance_animations(&mut self, ticks: f32) {
        let shown = self.page.shown().len();
        for anim in self.button_anim.iter_mut().take(shown) {
            if *anim > 0.0 {
                *anim -= ticks;
            }
        }
        for (index, tab) in self.data.tabs.iter().enumerate() {
            if self.tab_anim[index] > 0.0 && self.book.tab_visible(tab.category) {
                self.tab_anim[index] -= ticks;
            }
        }
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
            Tab::Category(index) => (
                self.data.tabs[index].primary,
                self.data.tabs[index].secondary,
            ),
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
