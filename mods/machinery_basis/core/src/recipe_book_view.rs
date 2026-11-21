//! O que a janela do livro mostra e como ela responde (`VirtualRecipeBookComponent`, `RecipeBookPage`, `MekanismRecipeButton`), sem
//! desenho: os dados de cada máquina (as abas e as linhas dos displays), o livro do cliente montado com o que o jogador desbloqueou,
//! a aba, a busca, o filtro, a página e o botão que cada item da grade mostra. Quem desenha e liga ao mouse e ao teclado é a cola
//! da `machinery_basis`. A lib não conhece nenhum id: as abas e as linhas chegam como dados.

use std::collections::HashMap;

use crate::recipe_book::{
    BUTTON_SIZE, ClientBook, Craftable, Entry, IMAGE_HEIGHT, IMAGE_WIDTH, Page, Rect, SEARCH_MAX_LENGTH, button_rect, contains,
    current_recipe_index, display_stack_index, slot_select_index,
};
use crate::recipe_place::{Ingredient, Recipe, Tank};
use crate::stacked_contents::StackedContents;

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

/// O que um display pede para ser feito: `(id, craftingRequirements, quantidades)`. `craftingRequirements` é a lista dos itens que
/// cada `Ingredient` aceita (`None` sem a lista); `quantidades` é, para um `MekanismRecipeDisplay`, o
/// `resolveForFirstStack(...).count()` de cada ingrediente do display (zero para químico e fluido), e `None` para os outros displays
/// (o `FurnaceRecipeDisplay` do Energized Smelter).
pub type Requirement = (
    i32,
    Option<&'static [&'static [&'static str]]>,
    Option<&'static [i32]>,
);

/// As pilhas que um ingrediente ou uma saída de um display mostra, uma por vez (`SlotDisplay.resolveForStacks`, ou o `resolve` com o
/// `ChemicalStackContentsFactory`): `(id, quantidade)`.
pub type Shown = &'static [(&'static str, i32)];

/// O que o fantasma de um display desenha: `(id, itens de cada ingrediente, itens de cada saída, químicos de cada ingrediente, químicos
/// de cada saída)`. Num `MekanismRecipeDisplay` são `ingredients()` e `rawOutputs()`; num `FurnaceRecipeDisplay`, `ingredient()` e
/// `result()`.
pub type GhostRow = (i32, &'static [Shown], &'static [Shown], &'static [Shown], &'static [Shown]);

/// O que o mod diz do livro de uma máquina: o tipo de livro (compartilhado entre as máquinas do mesmo tipo de receita), as abas
/// depois da bússola, e as linhas de todos os displays (a lista filtra pelo tipo).
#[derive(Debug)]
pub struct BookData {
    pub book_type: &'static str,
    /// O nome do `RecipeBookType` (`MEKANISM_ENRICHING`): a chave dos `RecipeBookSettings` e a origem dos campos do NBT do jogador.
    pub settings_type: &'static str,
    pub tabs: &'static [BookTab],
    pub rows: &'static [Row],
    /// Os requisitos de cada display, em ordem de id.
    pub requirements: &'static [Requirement],
    /// O que o fantasma de cada display desenha, em ordem de id.
    pub ghosts: &'static [GhostRow],
}

impl BookData {
    /// Os requisitos do display `id`.
    pub fn requirement(&self, id: i32) -> Option<&'static Requirement> {
        self.requirements
            .binary_search_by_key(&id, |requirement| requirement.0)
            .ok()
            .map(|index| &self.requirements[index])
    }

    /// O que o fantasma do display `id` desenha.
    pub fn ghost(&self, id: i32) -> Option<&'static GhostRow> {
        self.ghosts
            .binary_search_by_key(&id, |ghost| ghost.0)
            .ok()
            .map(|index| &self.ghosts[index])
    }

    /// A receita do display `id` como o `switch` de `PlaceMekanismRecipe.placeRecipe` a vê, com o tanque de químico da máquina em
    /// `tank` (o químico e a quantidade; `None` vazio). Um display que não é `MekanismRecipeDisplay` (sem as quantidades) é de uma
    /// receita que não é `MekanismRecipe`; com um ingrediente de químico é uma `ItemStackChemicalToObjectRecipe`, e o tanque tem de
    /// passar no `getChemicalInput().test(...)` (o químico certo, na quantidade que a receita pede ou mais).
    pub fn placement(&self, id: i32, tank: Option<(&str, i64)>) -> Option<Recipe<'static>> {
        let requirement = self.requirement(id)?;
        let Some(counts) = requirement.2 else {
            return Some(Recipe::Foreign);
        };
        let items = requirement.1?.first()?;
        let count = counts.iter().copied().find(|count| *count > 0)?;
        let ingredient = Ingredient { items, count };
        let chemical = self
            .ghost(id)
            .and_then(|ghost| ghost.3.iter().find(|shown| !shown.is_empty()));
        Some(match chemical {
            None => Recipe::Item(ingredient),
            Some(accepted) => Recipe::ItemChemical(
                ingredient,
                match tank {
                    None => Tank::Empty,
                    Some((stored, amount)) => {
                        if accepted
                            .iter()
                            .any(|(name, needed)| *name == stored && amount >= i64::from(*needed))
                        {
                            Tank::Matches
                        } else {
                            Tank::Mismatch
                        }
                    }
                },
            ),
        })
    }

    /// `MekanismRecipeBookComponent.canCraft` do display `id` com o que o jogador e a máquina têm.
    pub fn can_craft(&self, id: i32, contents: &StackedContents) -> bool {
        self.requirement(id)
            .is_some_and(|requirement| can_craft(requirement, contents))
    }
}

/// `MekanismRecipeBookComponent.canCraft(entry, stackedContents, context)`: sem a lista de requisitos não dá; com a lista vazia (só
/// químico ou fluido) dá; num `MekanismRecipeDisplay`, cada ingrediente de item entra tantas vezes quantas o display pede dele
/// (`scaledRequirements`), e um ingrediente a mais do que a lista tem responde que não (`Failed to determine if ... could be crafted`).
pub fn can_craft(requirement: &Requirement, contents: &StackedContents) -> bool {
    let Some(requirements) = requirement.1 else {
        return false;
    };
    if requirements.is_empty() {
        return true;
    }
    let Some(counts) = requirement.2 else {
        return contents.try_pick(requirements, 1);
    };
    let mut scaled: Vec<&[&str]> = Vec::new();
    let mut item_index = 0;
    for &required in counts {
        if required > 0 {
            let Some(ingredient) = requirements.get(item_index) else {
                return false;
            };
            item_index += 1;
            for _ in 0..required {
                scaled.push(ingredient);
            }
        }
    }
    contents.try_pick(&scaled, 1)
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

/// `GhostSlot.get(slotSelectTime.currentIndex())`: a pilha que o fantasma mostra agora (`elements.get(index % size)`).
pub fn ghost_shown(entries: Shown, select_index: i32) -> Option<(&'static str, i32)> {
    if entries.is_empty() {
        return None;
    }
    Some(entries[select_index.rem_euclid(entries.len() as i32) as usize])
}

/// `GhostSlot.createWithMaxTank`: a maior quantidade de químico (ou fluido) entre os ingredientes e as saídas do display.
pub fn ghost_max_tank(ghost: &GhostRow) -> i32 {
    ghost
        .3
        .iter()
        .chain(ghost.4)
        .flat_map(|shown| shown.iter().map(|(_, amount)| *amount))
        .max()
        .unwrap_or(0)
}

/// `GuiTankBar.renderBarGhostContents`: o nível que a barra desenha, entre um quarto e três quartos dela.
pub fn ghost_bar_level(amount: i32, max: i32) -> f64 {
    let scale = if max == 0 {
        0.0
    } else {
        f64::from(amount) / f64::from(max)
    };
    0.25 + scale / 2.0
}

/// Um botão do overlay das variantes (`OverlayRecipeComponent.OverlayRecipeButton`): o display, se dava para fazê-lo quando o overlay
/// abriu (o sprite) e onde fica (24 por 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayButton {
    pub id: i32,
    pub craftable: bool,
    pub rect: Rect,
}

/// O overlay das variantes de um botão (`OverlayRecipeComponent`), nas coordenadas em que o livro foi clicado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub x: i32,
    pub y: i32,
    pub buttons: Vec<OverlayButton>,
    /// `collection`: a coleção do botão, pelo índice no livro enquanto ela é a do livro (`None` depois que o livro refez as coleções: o
    /// Java continua com o objeto antigo).
    collection: Option<usize>,
    /// `collection.isCraftable(id)` de cada botão, como estava da última vez que a coleção foi conferida.
    craftable_now: Vec<bool>,
}

/// `OverlayRecipeComponent.BUTTON_SIZE`.
pub const OVERLAY_BUTTON_SIZE: i32 = 25;

impl Overlay {
    /// `maxRow`: quatro por linha até 16 botões, cinco com mais.
    fn max_row(total: usize) -> usize {
        if total <= 16 { 4 } else { 5 }
    }

    /// O fundo (`blitSprite(OVERLAY_RECIPE_SPRITE, x, y, width * 25 + 8, height * 25 + 8)`).
    pub fn background(&self) -> Rect {
        let max_row = Self::max_row(self.buttons.len());
        let width = self.buttons.len().min(max_row) as i32;
        let height = (self.buttons.len() as f32 / max_row as f32).ceil() as i32;
        [
            self.x,
            self.y,
            width * OVERLAY_BUTTON_SIZE + 8,
            height * OVERLAY_BUTTON_SIZE + 8,
        ]
    }

    /// `OverlayRecipeComponent.init`: as fazíveis primeiro, depois as outras (nenhuma com o filtro ligado), a partir do canto do botão
    /// (`button_x`, `button_y`) e empurrado para caber em volta do centro do livro, com as contas em `float` do Java.
    #[allow(clippy::too_many_arguments)]
    fn new(
        collection: usize,
        craftable: &[i32],
        uncraftable: &[i32],
        button_x: i32,
        button_y: i32,
        center_x: i32,
        center_y: i32,
        button_width: f32,
    ) -> Self {
        let craftables = craftable.len();
        let total = craftables + uncraftable.len();
        let max_row = Self::max_row(total);
        let rows = (total as f32 / max_row as f32).ceil() as i32;
        let mut x = button_x;
        let mut y = button_y;
        let right = (x + total.min(max_row) as i32 * OVERLAY_BUTTON_SIZE) as f32;
        let max_left = (center_x + 50) as f32;
        if right > max_left {
            x = (x as f32 - button_width * ((right - max_left) / button_width) as i32 as f32) as i32;
        }
        let bottom = (y + rows * OVERLAY_BUTTON_SIZE) as f32;
        let max_bottom = (center_y + 50) as f32;
        if bottom > max_bottom {
            y = (y as f32 - button_width * ((bottom - max_bottom) / button_width).ceil() as i32 as f32) as i32;
        }
        let top = y as f32;
        let max_top = (center_y - 100) as f32;
        if top < max_top {
            y = (y as f32 - button_width * ((top - max_top) / button_width).ceil() as i32 as f32) as i32;
        }
        let buttons: Vec<OverlayButton> = craftable
            .iter()
            .chain(uncraftable)
            .enumerate()
            .map(|(index, &id)| OverlayButton {
                id,
                craftable: index < craftables,
                rect: [
                    x + 4 + OVERLAY_BUTTON_SIZE * (index % max_row) as i32,
                    y + 5 + OVERLAY_BUTTON_SIZE * (index / max_row) as i32,
                    24,
                    24,
                ],
            })
            .collect();
        let craftable_now = buttons.iter().map(|button| button.craftable).collect();
        Self {
            x,
            y,
            buttons,
            collection: Some(collection),
            craftable_now,
        }
    }
}

/// O botão do mouse num clique da página do livro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    Left,
    Right,
    Other,
}

/// O que um clique na grade ou no overlay fez (`RecipeBookPage.mouseClicked` seguido de `RecipeBookComponent.mouseClicked`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageClick {
    /// Nada da grade sob o clique: ele segue para a caixa de busca, o filtro e as abas.
    Missed,
    /// O clique foi da página e não escolheu receita (`return true`); `sound` diz se um botão tocou o som dele.
    Handled { sound: bool },
    /// `tryPlaceRecipe` aceitou: a receita vai ao servidor (`handlePlaceRecipe`), e o som do botão toca.
    Place(i32),
    /// `tryPlaceRecipe` recusou (a mesma receita não fazível outra vez): `mouseClicked` devolve falso depois de o botão tocar o som.
    Refused,
}

/// O que a janela guarda entre uma abertura e outra, porque no Java é do componente da tela e não da janela: o relógio do
/// `SlotSelectTime`, a última receita colocada e a última clicada.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Carried {
    pub time: f32,
    last_placed: Option<i32>,
    last_recipe: Option<(i32, bool)>,
}

impl Carried {
    /// `VirtualRecipeBookComponent.slotClicked` de um slot de receita com a janela fechada: `lastPlacedRecipe` volta a nulo.
    pub fn slot_clicked(&mut self) {
        self.last_placed = None;
    }
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
    /// `lastPlacedRecipe`: a última receita que o livro mandou colocar.
    last_placed: Option<i32>,
    /// `lastRecipe` e `lastRecipeCollection`: a última receita clicada (a que o Enter repete), com o `isCraftable` dela e, enquanto a
    /// coleção for a do livro, o índice dela.
    last_recipe: Option<(i32, bool, Option<usize>)>,
    /// O overlay das variantes, quando aberto.
    pub overlay: Option<Overlay>,
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

/// O `ClientRecipeBook` de quem conhece `unlocked`: as coleções (`rebuildCollections`), o que a busca lê de cada uma e o
/// `selectMatchingRecipes` com o `can_craft`.
fn build_book(
    data: &'static BookData,
    rows: &HashMap<i32, &'static Row>,
    unlocked: impl Iterator<Item = i32>,
    highlighted: impl Iterator<Item = i32>,
    can_craft: &dyn Fn(&Row) -> bool,
) -> ClientBook {
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
    book
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
        let book = build_book(data, &rows, unlocked, highlighted, can_craft);
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
            last_placed: None,
            last_recipe: None,
            overlay: None,
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

    /// `RecipeBookComponent.recipesUpdated`: o livro do jogador mudou (o servidor mandou receitas ou tirou). As coleções são refeitas
    /// e a lista da página também, sem voltar à primeira página (`updateCollections(false, ...)`).
    pub fn update_book(
        &mut self,
        unlocked: impl Iterator<Item = i32>,
        highlighted: impl Iterator<Item = i32>,
        can_craft: &dyn Fn(&Row) -> bool,
    ) {
        self.book = build_book(self.data, &self.rows, unlocked, highlighted, can_craft);
        // As coleções são objetos novos: a última clicada e o overlay ficam com as antigas, que ninguém confere mais.
        if let Some(last) = self.last_recipe.as_mut() {
            last.2 = None;
        }
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.collection = None;
        }
        self.refresh(false);
    }

    /// `RecipeBookComponent.updateStackedContents`: o inventário mudou, então o que dá para fazer é conferido de novo
    /// (`selectMatchingRecipes`) e a lista refeita sem voltar a página.
    pub fn reselect(&mut self, can_craft: &dyn Fn(&Row) -> bool) {
        for index in 0..self.book.collection_count() {
            let rows = &self.rows;
            self.book
                .collection_mut(index)
                .select(|_| true, |entry| can_craft(rows[&entry.id]));
        }
        if let Some((id, craftable, Some(collection))) = self.last_recipe.as_mut() {
            *craftable = self.book.collection(*collection).is_craftable(*id);
        }
        if let Some(overlay) = self.overlay.as_mut()
            && let Some(collection) = overlay.collection
        {
            let collection = self.book.collection(collection);
            overlay.craftable_now = overlay
                .buttons
                .iter()
                .map(|button| collection.is_craftable(button.id))
                .collect();
        }
        self.refresh(false);
    }

    /// O que passa de uma abertura da janela para a outra.
    pub fn carried(&self) -> Carried {
        Carried {
            time: self.time,
            last_placed: self.last_placed,
            last_recipe: self.last_recipe.map(|(id, craftable, _)| (id, craftable)),
        }
    }

    /// A janela reabre na mesma tela: o relógio e as últimas receitas continuam de onde estavam.
    pub fn restore(&mut self, carried: &Carried) {
        self.time = carried.time;
        self.last_placed = carried.last_placed;
        self.last_recipe = carried.last_recipe.map(|(id, craftable)| {
            let collection = (0..self.book.collection_count())
                .find(|&index| self.book.collection(index).entries.iter().any(|entry| entry.id == id));
            (id, craftable, collection)
        });
    }

    /// As receitas escolhidas do botão `slot` da página, com o estado do filtro (`MekanismRecipeButton.init`).
    fn chosen(&self, slot: usize) -> Option<(usize, Vec<i32>)> {
        let index = *self.page.shown().get(slot)?;
        let status = if self.filtering {
            Craftable::Craftable
        } else {
            Craftable::Any
        };
        let ids: Vec<i32> = self
            .book
            .collection(index)
            .selected_recipes(status)
            .iter()
            .map(|entry| entry.id)
            .collect();
        (!ids.is_empty()).then_some((index, ids))
    }

    /// `RecipeBookComponent.tryPlaceRecipe` e o que o segue em `mouseClicked`: a mesma receita não fazível não vai de novo; a que vai
    /// vira a última colocada e a última clicada.
    fn try_place(&mut self, id: i32, craftable: bool, collection: Option<usize>) -> bool {
        if !craftable && self.last_placed == Some(id) {
            return false;
        }
        self.last_placed = Some(id);
        self.last_recipe = Some((id, craftable, collection));
        true
    }

    /// `VirtualRecipeBookComponent.slotClicked` de um slot de receita: `lastPlacedRecipe` volta a nulo.
    pub fn slot_clicked(&mut self) {
        self.last_placed = None;
    }

    /// `RecipeBookPage.mouseClicked` depois das setas, e o `tryPlaceRecipe` do componente: com o overlay aberto, o esquerdo num botão
    /// dele escolhe a receita e qualquer outro clique o fecha; sem ele, o esquerdo num botão da grade escolhe a receita que ele mostra
    /// agora (`getCurrentRecipe`) e o direito abre o overlay de um botão com mais de uma (`isOnlyOption`). `x` e `y` são do clique, e
    /// (`xo`, `yo`) a origem do livro, no mesmo espaço.
    pub fn click_page(&mut self, xo: i32, yo: i32, x: f64, y: f64, mouse: Mouse) -> PageClick {
        if let Some(overlay) = self.overlay.as_ref() {
            let hit = (mouse == Mouse::Left)
                .then(|| {
                    overlay
                        .buttons
                        .iter()
                        .position(|button| contains(button.rect, x, y))
                })
                .flatten();
            let Some(hit) = hit else {
                self.overlay = None;
                return PageClick::Handled { sound: false };
            };
            let (id, craftable, collection) = (
                overlay.buttons[hit].id,
                overlay.craftable_now[hit],
                overlay.collection,
            );
            return if self.try_place(id, craftable, collection) {
                PageClick::Place(id)
            } else {
                PageClick::Refused
            };
        }
        self.click_grid(xo, yo, x, y, mouse)
    }

    fn click_grid(&mut self, xo: i32, yo: i32, x: f64, y: f64, mouse: Mouse) -> PageClick {
        if mouse == Mouse::Other {
            return PageClick::Missed;
        }
        let shown = self.page.shown().len();
        let Some(slot) = (0..shown).find(|&slot| contains(button_rect(xo, yo, slot), x, y)) else {
            return PageClick::Missed;
        };
        let Some((collection, ids)) = self.chosen(slot) else {
            return PageClick::Missed;
        };
        if mouse == Mouse::Left {
            let id = ids[current_recipe_index(slot_select_index(self.time), ids.len())];
            let craftable = self.book.collection(collection).is_craftable(id);
            return if self.try_place(id, craftable, Some(collection)) {
                PageClick::Place(id)
            } else {
                PageClick::Refused
            };
        }
        if ids.len() > 1 {
            let found = self.book.collection(collection);
            let craftable: Vec<i32> = found
                .selected_recipes(Craftable::Craftable)
                .iter()
                .map(|entry| entry.id)
                .collect();
            let uncraftable: Vec<i32> = if self.filtering {
                Vec::new()
            } else {
                found
                    .selected_recipes(Craftable::NotCraftable)
                    .iter()
                    .map(|entry| entry.id)
                    .collect()
            };
            let rect = button_rect(xo, yo, slot);
            self.overlay = Some(Overlay::new(
                collection,
                &craftable,
                &uncraftable,
                rect[0],
                rect[1],
                xo + IMAGE_WIDTH / 2,
                yo + 13 + IMAGE_HEIGHT / 2,
                BUTTON_SIZE as f32,
            ));
        }
        PageClick::Handled { sound: true }
    }

    /// O Enter, o Enter do teclado numérico e o espaço fora da caixa de busca (`event.isSelection()` com `lastRecipe`): o som do botão
    /// toca e a última receita clicada vai de novo, se o `tryPlaceRecipe` deixar. `None` sem última receita (a tecla não é do livro).
    pub fn place_last(&mut self) -> Option<PageClick> {
        let (id, craftable, collection) = self.last_recipe?;
        Some(if self.try_place(id, craftable, collection) {
            PageClick::Place(id)
        } else {
            PageClick::Refused
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_is_pushed_back_towards_the_middle_of_the_book_as_the_original_does() {
        // O botão da última coluna (x = 11 + 4 * 25) com quatro variantes: a borda direita (211) passa de `centerX + 50` (123) por 88,
        // e o overlay recua `25 * (int) (88 / 25)` = 75.
        let overlay = Overlay::new(0, &[1, 2, 3, 4], &[], 111, 41, 73, 106, 25.0);
        assert_eq!((overlay.x, overlay.y), (36, 41));
        assert_eq!(overlay.background(), [36, 41, 108, 33]);
        assert_eq!(overlay.buttons[0].rect, [40, 46, 24, 24]);
        assert_eq!(overlay.buttons[3].rect, [115, 46, 24, 24]);
        assert!(overlay.buttons.iter().all(|button| button.craftable));
        // Seis variantes em duas linhas a partir da última fila: a borda de baixo (166) passa de `centerY + 50` (156) por 10, e o
        // overlay sobe `25 * ceil(10 / 25)` = 25. As que não dão para fazer vêm depois.
        let overlay = Overlay::new(0, &[1, 2, 3, 4], &[5, 6], 11, 116, 73, 106, 25.0);
        assert_eq!((overlay.x, overlay.y), (11, 91));
        assert_eq!(overlay.background(), [11, 91, 108, 58]);
        assert_eq!(overlay.buttons[4], OverlayButton { id: 5, craftable: false, rect: [15, 121, 24, 24] });
        // Com mais de 16 as linhas têm cinco; uma sobra menor que um botão não recua (`(int) (13 / 25)` é zero).
        let many: Vec<i32> = (0..17).collect();
        let overlay = Overlay::new(0, &many, &[], 11, 31, 73, 106, 25.0);
        assert_eq!((overlay.x, overlay.y), (11, 31));
        assert_eq!(overlay.background(), [11, 31, 133, 108]);
        assert_eq!(overlay.buttons[16].rect, [40, 111, 24, 24]);
    }

    #[test]
    fn the_ghost_cycles_its_entries_and_the_bar_fills_between_a_quarter_and_three_quarters() {
        let entries: Shown = &[("a", 3), ("b", 3)];
        assert_eq!(ghost_shown(entries, 0), Some(("a", 3)));
        assert_eq!(ghost_shown(entries, 3), Some(("b", 3)));
        assert_eq!(ghost_shown(&[], 1), None);
        let ghost: GhostRow = (1, &[&[("a", 1)], &[]], &[&[("c", 1)]], &[&[], &[("x", 10)]], &[&[]]);
        assert_eq!(ghost_max_tank(&ghost), 10);
        assert_eq!(ghost_bar_level(10, 10), 0.75);
        assert_eq!(ghost_bar_level(5, 10), 0.5);
        assert_eq!(ghost_bar_level(5, 0), 0.25);
    }
}
