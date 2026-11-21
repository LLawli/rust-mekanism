//! O livro de receitas do vanilla que o `GuiRecipeBookWindow` do Mekanism hospeda, só na parte que é conta: as coleções do
//! `ClientRecipeBook`, o `RecipeCollection`, a lista que o `RecipeBookComponent.updateCollections` monta (receitas escolhidas,
//! busca, filtro), a paginação do `RecipeBookPage` e as posições de cada peça. Nada aqui desenha nem conhece um id, um químico ou
//! uma máquina: os dados (as receitas, os textos que a busca lê, o que a máquina diz que casa) entram por parâmetro.
//!
//! O que o Horizon já tem (`horizon-client/src/recipe_book.rs` e `game/recipe_book.rs`) é o livro do vanilla das quatro telas
//! dele (`BookType`: grade de montagem e as três fornalhas), preso a elas: `active_book` devolve `None` para a tela de um mod
//! (`A mod's screen has no book`), e a API 378 só valida a categoria de um tipo de receita de mod ("This does not yet add tabs
//! or book displays"). Por isso a janela é daqui.
use std::collections::{BTreeSet, HashMap};

use crate::sim::java_hash::JavaIntMap;

/// `RecipeBookComponent.IMAGE_WIDTH` e `IMAGE_HEIGHT`: o fundo `recipe_book.png` sem as abas.
pub const IMAGE_WIDTH: i32 = 147;
pub const IMAGE_HEIGHT: i32 = 166;
/// `GuiRecipeBookWindow.Y_OFFSET`: o título da janela ocupa os 10 de cima, e o livro começa abaixo.
pub const Y_OFFSET: i32 = 10;
/// `GuiRecipeBookWindow.HEIGHT`.
pub const WINDOW_HEIGHT: i32 = IMAGE_HEIGHT + Y_OFFSET;
/// `GuiRecipeBookWindow.WIDTH_WITH_TABS`: as abas ficam 30 à esquerda do fundo.
pub const WIDTH_WITH_TABS: i32 = IMAGE_WIDTH + 30;
/// `RecipeBookPage.ITEMS_PER_PAGE`.
pub const ITEMS_PER_PAGE: usize = 20;
/// Os botões de receita ficam em cinco colunas.
pub const COLUMNS: usize = 5;
/// `RecipeButton`: 25 por 25.
pub const BUTTON_SIZE: i32 = 25;
/// `RecipeBookTabButton`: 35 por 27, e `updateTabs` os põe 27 abaixo um do outro.
pub const TAB_WIDTH: i32 = 35;
pub const TAB_HEIGHT: i32 = 27;
/// `RecipeBookComponent.initVisuals`: `new EditBox(font, xo + 25, yo + 13, 81, 9 + 5, ...)`, com `setMaxLength(50)`.
pub const SEARCH_MAX_LENGTH: usize = 50;
/// `RecipeBookComponent.TICKS_TO_SWAP_SLOT`: `SlotSelectTime` é `floor(time / 30)`.
pub const SLOT_SWAP_TICKS: f32 = 30.0;

/// Um retângulo `[x, y, largura, altura]` em pixels de GUI.
pub type Rect = [i32; 4];

/// `RecipeBookPage.init`: o botão `i` da página, em cinco colunas de 25 a partir de (`xo + 11`, `yo + 31`).
pub fn button_rect(xo: i32, yo: i32, index: usize) -> Rect {
    let (column, row) = ((index % COLUMNS) as i32, (index / COLUMNS) as i32);
    [
        xo + 11 + 25 * column,
        yo + 31 + 25 * row,
        BUTTON_SIZE,
        BUTTON_SIZE,
    ]
}

/// `new ImageButton(xo + 93, yo + 137, 12, 17, ...)`: a seta para a página seguinte.
pub fn forward_rect(xo: i32, yo: i32) -> Rect {
    [xo + 93, yo + 137, 12, 17]
}

/// `new ImageButton(xo + 38, yo + 137, 12, 17, ...)`: a seta para a página anterior.
pub fn back_rect(xo: i32, yo: i32) -> Rect {
    [xo + 38, yo + 137, 12, 17]
}

/// A caixa de busca: `xo + 25, yo + 13, 81, 9 + 5`.
pub fn search_rect(xo: i32, yo: i32) -> Rect {
    [xo + 25, yo + 13, 81, 14]
}

/// `magnifierIconPlacement`: `ScreenRectangle.of(HORIZONTAL, xo + 8, searchBox.getY(), searchBox.getX() - xOrigin, searchBox.getHeight())`.
/// O clique nela foca a caixa (`clickedMagnifierIcon`).
pub fn magnifier_rect(xo: i32, yo: i32) -> Rect {
    let search = search_rect(xo, yo);
    [xo + 8, search[1], search[0] - xo, search[3]]
}

/// O botão de filtro: `create(xo + 110, yo + 12, 26, 16, ...)`.
pub fn filter_rect(xo: i32, yo: i32) -> Rect {
    [xo + 110, yo + 12, 26, 16]
}

/// `VirtualRecipeBookComponent.updateTabs`: a aba visível de índice `index`, em (`xo - 30`, `yo + 3 + 27 * index`).
pub fn tab_rect(xo: i32, yo: i32, index: usize) -> Rect {
    [
        xo - 30,
        yo + 3 + TAB_HEIGHT * index as i32,
        TAB_WIDTH,
        TAB_HEIGHT,
    ]
}

/// `RecipeBookPage.extractRenderState`: o texto `Page n/m` centrado em `xo + 73`, em `yo + 141`.
pub fn page_text_x(xo: i32, text_width: i32) -> i32 {
    xo - text_width / 2 + 73
}

/// Se o ponto cai dentro do retângulo, como `AbstractWidget.isMouseOver` (`>=` na esquerda e em cima, `<` na direita e embaixo).
pub fn contains(rect: Rect, x: f64, y: f64) -> bool {
    x >= f64::from(rect[0])
        && x < f64::from(rect[0] + rect[2])
        && y >= f64::from(rect[1])
        && y < f64::from(rect[1] + rect[3])
}

/// `RecipeDisplayEntry`: o que o livro sabe de uma receita. `id` é o `RecipeDisplayId`; `group` é o `OptionalInt` do grupo, que
/// junta no mesmo botão as receitas de mesmo grupo e categoria.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: i32,
    pub category: String,
    pub group: Option<i32>,
}

/// `RecipeCollection.CraftableStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Craftable {
    Any,
    Craftable,
    NotCraftable,
}

/// `RecipeCollection`: as receitas que o botão agrupa e, de cada uma, se ela é a que a máquina aceita (`selected`) e se dá para
/// fazê-la com o que o jogador tem (`craftable`).
#[derive(Clone, Debug, Default)]
pub struct Collection {
    pub entries: Vec<Entry>,
    pub selected: BTreeSet<i32>,
    pub craftable: BTreeSet<i32>,
    /// Os textos que a busca lê (`getTooltipLines` do resultado de cada receita, sem formatação e sem espaço nas pontas),
    /// já em minúsculas (`toLowerCase(Locale.ROOT)`).
    pub names: Vec<String>,
    /// Os ids de resultado (`getResultIds`) como (espaço de nomes, caminho), em minúsculas.
    pub ids: Vec<(String, String)>,
}

impl Collection {
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            ..Self::default()
        }
    }

    /// `MekanismRecipeBookComponent.selectMatchingRecipes(collection, stackedContents, selector)`: para cada receita, `selected` diz o
    /// `selector` e `craftable` é `selected` e o `can_craft`. Uma receita que deixa de ser escolhida sai dos dois conjuntos.
    pub fn select(
        &mut self,
        selector: impl Fn(&Entry) -> bool,
        can_craft: impl Fn(&Entry) -> bool,
    ) {
        for entry in &self.entries {
            let is_selected = selector(entry);
            if is_selected {
                self.selected.insert(entry.id);
            } else {
                self.selected.remove(&entry.id);
            }
            if is_selected && can_craft(entry) {
                self.craftable.insert(entry.id);
            } else {
                self.craftable.remove(&entry.id);
            }
        }
    }

    pub fn is_craftable(&self, id: i32) -> bool {
        self.craftable.contains(&id)
    }

    pub fn has_craftable(&self) -> bool {
        !self.craftable.is_empty()
    }

    pub fn has_any_selected(&self) -> bool {
        !self.selected.is_empty()
    }

    /// `getSelectedRecipes(selector)`: na ordem de `entries`.
    pub fn selected_recipes(&self, status: Craftable) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| match status {
                Craftable::Any => self.selected.contains(&entry.id),
                Craftable::Craftable => self.craftable.contains(&entry.id),
                Craftable::NotCraftable => {
                    self.selected.contains(&entry.id) && !self.craftable.contains(&entry.id)
                }
            })
            .collect()
    }

    /// A busca do livro (`FullTextSearchTree`, que `SessionSearchTrees.recipes()` monta com os textos do resultado e os ids dele).
    /// `query` já em minúsculas. Sem `:` casa o texto que algum nome contém. Com `:`, o espaço de nomes antes dos dois pontos tem de
    /// estar num id e o resto (sem espaço nas pontas) num caminho de id ou num nome (`IdSearchTree.search`).
    pub fn matches_search(&self, query: &str) -> bool {
        match query.find(':') {
            None => self.names.iter().any(|name| name.contains(query)),
            Some(colon) => {
                let (namespace, path) = (query[..colon].trim(), query[colon + 1..].trim());
                self.ids.iter().any(|(ns, _)| ns.contains(namespace))
                    && (self.ids.iter().any(|(_, p)| p.contains(path))
                        || self.names.iter().any(|name| name.contains(path)))
            }
        }
    }
}

/// `ClientRecipeBook`: as receitas que o jogador conhece e as coleções por aba. `known` é o `HashMap<RecipeDisplayId, ...>` do
/// original, com todos os displays que o jogador conhece, de qualquer receita do jogo: os que não são deste livro
/// ([`ClientBook::add_other`]) ocupam a tabela, e com isso decidem o tamanho dela e a ordem das coleções, sem entrar em aba nenhuma.
#[derive(Clone, Debug, Default)]
pub struct ClientBook {
    known: JavaIntMap<Option<Entry>>,
    highlight: BTreeSet<i32>,
    collections: Vec<Collection>,
    by_tab: HashMap<String, Vec<usize>>,
}

impl ClientBook {
    /// `add`: o `HashMap` troca a entrada de mesmo id sem mudar a posição dela na tabela.
    pub fn add(&mut self, entry: Entry) {
        self.known.put(entry.id, Some(entry));
    }

    /// O livro sobre uma tabela de `known` já montada: cada id com a linha dele, ou sem linha se o display é de outro livro.
    pub fn with_known(known: JavaIntMap<Option<Entry>>) -> Self {
        Self {
            known,
            ..Self::default()
        }
    }

    /// `add` de um display que este livro não mostra (o de outra receita do jogo).
    pub fn add_other(&mut self, id: i32) {
        self.known.put(id, None);
    }

    pub fn remove(&mut self, id: i32) {
        self.known.remove(id);
        self.highlight.remove(&id);
    }

    /// `clear`: a tabela do `HashMap` fica do tamanho que tinha.
    pub fn clear(&mut self) {
        self.known.clear();
        self.highlight.clear();
    }

    /// Quantos displays o jogador conhece (`known.size()`), os de outros livros também.
    pub fn known_count(&self) -> usize {
        self.known.len()
    }

    /// O tamanho da tabela de `known` (`table.length`).
    pub fn known_capacity(&self) -> usize {
        self.known.capacity()
    }

    /// Os ids de `known`, na ordem da tabela (`known.values()`).
    pub fn known_order(&self) -> impl Iterator<Item = i32> + '_ {
        self.known.iter().map(|(id, _)| id)
    }

    pub fn will_highlight(&self, id: i32) -> bool {
        self.highlight.contains(&id)
    }

    pub fn add_highlight(&mut self, id: i32) {
        self.highlight.insert(id);
    }

    pub fn remove_highlight(&mut self, id: i32) {
        self.highlight.remove(&id);
    }

    /// `rebuildCollections`: `known.values()` na ordem da tabela do `HashMap<RecipeDisplayId, ...>` (o `hashCode` do record de um
    /// `int` é o `int`), cada receita sem grupo numa coleção só e as de mesmo (categoria, grupo) juntas na coleção da primeira; depois
    /// cada categoria de busca soma, em ordem, as coleções (as mesmas, não cópias) das categorias que inclui.
    /// `search_categories` é a lista de (categoria de busca, categorias incluídas).
    pub fn rebuild_collections(&mut self, search_categories: &[(String, Vec<String>)]) {
        let mut groups: HashMap<(String, i32), usize> = HashMap::new();
        let mut collections: Vec<Collection> = Vec::new();
        let mut by_tab: HashMap<String, Vec<usize>> = HashMap::new();
        for (_, entry) in self.known.iter() {
            let Some(entry) = entry else {
                continue;
            };
            match entry.group {
                None => {
                    by_tab
                        .entry(entry.category.clone())
                        .or_default()
                        .push(collections.len());
                    collections.push(Collection::new(vec![entry.clone()]));
                }
                Some(group) => match groups.get(&(entry.category.clone(), group)) {
                    Some(&index) => collections[index].entries.push(entry.clone()),
                    None => {
                        groups.insert((entry.category.clone(), group), collections.len());
                        by_tab
                            .entry(entry.category.clone())
                            .or_default()
                            .push(collections.len());
                        collections.push(Collection::new(vec![entry.clone()]));
                    }
                },
            }
        }
        for (search, included) in search_categories {
            let merged: Vec<usize> = included
                .iter()
                .flat_map(|category| by_tab.get(category).cloned().unwrap_or_default())
                .collect();
            by_tab.insert(search.clone(), merged);
        }
        self.collections = collections;
        self.by_tab = by_tab;
    }

    /// `getCollection(category)`: os índices em [`ClientBook::collection`], na ordem da aba.
    pub fn tab(&self, category: &str) -> &[usize] {
        self.by_tab.get(category).map_or(&[], Vec::as_slice)
    }

    /// Quantas coleções o livro tem (`getCollections().size()`).
    pub fn collection_count(&self) -> usize {
        self.collections.len()
    }

    pub fn collection(&self, index: usize) -> &Collection {
        &self.collections[index]
    }

    pub fn collection_mut(&mut self, index: usize) -> &mut Collection {
        &mut self.collections[index]
    }

    /// `RecipeBookTabButton.updateVisibility`: a aba aparece se alguma coleção dela tem receita escolhida.
    pub fn tab_visible(&self, category: &str) -> bool {
        self.tab(category)
            .iter()
            .any(|&index| self.collections[index].has_any_selected())
    }

    /// `RecipeBookTabButton.startAnimation`: a aba pisca se alguma receita que ela mostraria está para ser destacada.
    pub fn tab_highlighted(&self, category: &str, filtering: bool) -> bool {
        let status = if filtering {
            Craftable::Craftable
        } else {
            Craftable::Any
        };
        self.tab(category).iter().any(|&index| {
            self.collections[index]
                .selected_recipes(status)
                .iter()
                .any(|entry| self.highlight.contains(&entry.id))
        })
    }

    /// `RecipeBookComponent.updateCollections`: as coleções da aba, tiradas as sem receita escolhida, as que a busca não acha (só se
    /// há texto) e, filtrando, as sem receita que dê para fazer. `search_matches` é o que a busca devolve: o do [`Collection::matches_search`]
    /// sobre o texto em minúsculas.
    pub fn visible_collections(&self, category: &str, search: &str, filtering: bool) -> Vec<usize> {
        let query = search.to_lowercase();
        self.tab(category)
            .iter()
            .copied()
            .filter(|&index| self.collections[index].has_any_selected())
            .filter(|&index| query.is_empty() || self.collections[index].matches_search(&query))
            .filter(|&index| !filtering || self.collections[index].has_craftable())
            .collect()
    }
}

/// `RecipeBookPage`: a página atual das coleções que o componente mandou.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub collections: Vec<usize>,
    pub total_pages: usize,
    pub current_page: usize,
    pub filtering: bool,
}

impl Page {
    /// `updateCollections(list, resetPage, isFiltering)`: `totalPages = ceil(size / 20.0)`; a página volta a 0 se pediram ou se ela
    /// deixou de existir.
    pub fn update_collections(
        &mut self,
        collections: Vec<usize>,
        reset_page: bool,
        filtering: bool,
    ) {
        self.total_pages = collections.len().div_ceil(ITEMS_PER_PAGE);
        self.collections = collections;
        self.filtering = filtering;
        if self.total_pages <= self.current_page || reset_page {
            self.current_page = 0;
        }
    }

    /// As coleções que os 20 botões mostram (`updateButtonsForPage`): o botão `i` mostra `collections[20 * page + i]`; os que sobram
    /// ficam invisíveis.
    pub fn shown(&self) -> &[usize] {
        let start = (ITEMS_PER_PAGE * self.current_page).min(self.collections.len());
        let end = (start + ITEMS_PER_PAGE).min(self.collections.len());
        &self.collections[start..end]
    }

    /// `updateArrowButtons`: a seta para a frente aparece com mais de uma página e antes da última.
    pub fn forward_visible(&self) -> bool {
        self.total_pages > 1 && self.current_page + 1 < self.total_pages
    }

    /// A seta para trás aparece com mais de uma página e depois da primeira.
    pub fn back_visible(&self) -> bool {
        self.total_pages > 1 && self.current_page > 0
    }

    /// O clique na seta para a frente (`forwardButton.mouseClicked`, só se ela está visível).
    pub fn forward(&mut self) -> bool {
        if self.forward_visible() {
            self.current_page += 1;
            true
        } else {
            false
        }
    }

    pub fn back(&mut self) -> bool {
        if self.back_visible() {
            self.current_page -= 1;
            true
        } else {
            false
        }
    }

    /// O texto `Page n/m`, só com mais de uma página (`gui.recipebook.page`).
    pub fn page_label(&self) -> Option<(usize, usize)> {
        (self.total_pages > 1).then_some((self.current_page + 1, self.total_pages))
    }
}

/// `DeltaTracker.Timer.advanceGameTime`: os ticks de um quadro de `elapsed_ms` milissegundos, que o livro soma ao `time` e tira das
/// animações (`extractRenderState(..., a)` com `getGameTimeDeltaTicks`). O tick tem os 50 ms de sempre, ou os do nível quando ele anda
/// normalmente e é mais lento (`Minecraft.getTickTargetMillis`: `max(50, millisecondsPerTick)`; parado ou em passos, 50).
pub fn delta_ticks(elapsed_ms: i64, runs_normally: bool, level_mspt: f32) -> f32 {
    let target = if runs_normally {
        50.0_f32.max(level_mspt)
    } else {
        50.0
    };
    elapsed_ms as f32 / target
}

/// `SlotSelectTime`: `Mth.floor(time / 30.0F)`, com o `time` que cresce a cada quadro sem o Ctrl.
pub fn slot_select_index(time: f32) -> i32 {
    (time / SLOT_SWAP_TICKS).floor() as i32
}

/// `RecipeButton.getCurrentRecipe`: a receita que o clique manda, a de índice `currentIndex % n` entre as escolhidas.
pub fn current_recipe_index(select_index: i32, selected: usize) -> usize {
    select_index.rem_euclid(selected as i32) as usize
}

/// `RecipeButton.getDisplayStack`: o par (receita, item dentro do resultado dela) que o botão mostra agora.
pub fn display_stack_index(select_index: i32, entries: usize) -> (usize, i32) {
    let offset = select_index / entries as i32;
    (
        select_index.wrapping_sub(entries as i32 * offset) as usize,
        offset,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_frame_counts_ticks_by_the_level_tick_when_it_is_slower() {
        // 20 por segundo: 100 ms são 2 ticks.
        assert_eq!(super::delta_ticks(100, true, 50.0), 2.0);
        // `/tick rate 10`: o tick tem 100 ms e o mesmo quadro vale 1.
        assert_eq!(super::delta_ticks(100, true, 100.0), 1.0);
        // Mais rápido que 20 não encurta o tick do cliente.
        assert_eq!(super::delta_ticks(100, true, 25.0), 2.0);
        // Parado (`/tick freeze`): 50 ms.
        assert_eq!(super::delta_ticks(100, false, 100.0), 2.0);
    }

    use super::*;

    fn entry(id: i32, category: &str, group: Option<i32>) -> Entry {
        Entry {
            id,
            category: category.to_owned(),
            group,
        }
    }

    fn book(entries: &[Entry]) -> ClientBook {
        let mut book = ClientBook::default();
        for entry in entries {
            book.add(entry.clone());
        }
        book.rebuild_collections(&[("search".to_owned(), vec!["a".to_owned(), "b".to_owned()])]);
        book
    }

    #[test]
    fn the_buttons_sit_in_five_columns_of_25_from_11_and_31() {
        assert_eq!(button_rect(100, 50, 0), [111, 81, 25, 25]);
        assert_eq!(button_rect(100, 50, 4), [211, 81, 25, 25]);
        assert_eq!(button_rect(100, 50, 5), [111, 106, 25, 25]);
        assert_eq!(button_rect(100, 50, 19), [211, 156, 25, 25]);
    }

    #[test]
    fn the_pieces_of_the_book_sit_where_the_component_puts_them() {
        assert_eq!(search_rect(10, 20), [35, 33, 81, 14]);
        assert_eq!(magnifier_rect(10, 20), [18, 33, 25, 14]);
        assert_eq!(filter_rect(10, 20), [120, 32, 26, 16]);
        assert_eq!(forward_rect(10, 20), [103, 157, 12, 17]);
        assert_eq!(back_rect(10, 20), [48, 157, 12, 17]);
        assert_eq!(tab_rect(10, 20, 0), [-20, 23, 35, 27]);
        assert_eq!(tab_rect(10, 20, 3), [-20, 104, 35, 27]);
        assert_eq!(page_text_x(10, 31), 10 - 15 + 73);
    }

    #[test]
    fn a_receita_sem_grupo_ganha_uma_colecao_e_as_de_um_grupo_dividem_a_primeira() {
        // A ordem do `HashMap` com ids pequenos é a numérica: 1, 2, 3, 4.
        let book = book(&[
            entry(1, "a", None),
            entry(2, "a", Some(7)),
            entry(3, "a", Some(7)),
            entry(4, "b", Some(7)),
        ]);
        let a: Vec<Vec<i32>> = book
            .tab("a")
            .iter()
            .map(|&i| book.collection(i).entries.iter().map(|e| e.id).collect())
            .collect();
        assert_eq!(a, vec![vec![1], vec![2, 3]]);
        // O mesmo grupo em outra categoria é outra coleção.
        let b: Vec<Vec<i32>> = book
            .tab("b")
            .iter()
            .map(|&i| book.collection(i).entries.iter().map(|e| e.id).collect())
            .collect();
        assert_eq!(b, vec![vec![4]]);
    }

    #[test]
    fn a_categoria_de_busca_soma_as_mesmas_colecoes_na_ordem() {
        let mut book = book(&[
            entry(1, "b", None),
            entry(2, "a", None),
            entry(3, "a", None),
        ]);
        let search: Vec<i32> = book
            .tab("search")
            .iter()
            .map(|&i| book.collection(i).entries[0].id)
            .collect();
        assert_eq!(search, vec![2, 3, 1]);
        // São as mesmas coleções: escolher por uma aba vale na outra.
        let first = book.tab("a")[0];
        book.collection_mut(first).select(|_| true, |_| false);
        assert!(book.collection(book.tab("search")[0]).has_any_selected());
    }

    #[test]
    fn a_ordem_das_colecoes_e_a_da_tabela_do_hashmap_e_nao_a_de_insercao() {
        // Ids que caem em baldes trocados numa tabela de 16: 17 vai ao balde 1 e 2 ao balde 2, 16 ao 0.
        let book = book(&[
            entry(17, "a", None),
            entry(2, "a", None),
            entry(16, "a", None),
        ]);
        let order: Vec<i32> = book
            .tab("a")
            .iter()
            .map(|&i| book.collection(i).entries[0].id)
            .collect();
        assert_eq!(order, vec![16, 17, 2]);
    }

    #[test]
    fn selecionar_marca_as_receitas_e_o_que_da_para_fazer_e_desmarca_o_resto() {
        let mut collection = Collection::new(vec![
            entry(1, "a", None),
            entry(2, "a", None),
            entry(3, "a", None),
        ]);
        collection.select(|e| e.id != 3, |e| e.id == 2 || e.id == 3);
        assert_eq!(
            collection
                .selected_recipes(Craftable::Any)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            collection
                .selected_recipes(Craftable::Craftable)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(
            collection
                .selected_recipes(Craftable::NotCraftable)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        // A 3 não é escolhida, então não é "dá para fazer" mesmo que o teste diga que sim.
        assert!(!collection.is_craftable(3));
        collection.select(|_| false, |_| true);
        assert!(!collection.has_any_selected() && !collection.has_craftable());
    }

    #[test]
    fn a_lista_da_aba_tira_o_que_nao_e_escolhido_o_que_a_busca_nao_acha_e_o_que_nao_da_para_fazer()
    {
        let mut book = book(&[
            entry(1, "a", None),
            entry(2, "a", None),
            entry(3, "a", None),
        ]);
        for (index, name, craftable) in [
            (0, "iron dust", true),
            (1, "gold dust", false),
            (2, "copper dust", true),
        ] {
            let index = book.tab("a")[index];
            let collection = book.collection_mut(index);
            collection.names = vec![name.to_owned()];
            let id = collection.entries[0].id;
            collection.select(|e| e.id != 3, move |_| craftable && id != 3);
        }
        let ids = |list: Vec<usize>| {
            list.iter()
                .map(|&i| book.collection(i).entries[0].id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(book.visible_collections("a", "", false)), vec![1, 2]);
        assert_eq!(ids(book.visible_collections("a", "GOLD", false)), vec![2]);
        assert_eq!(ids(book.visible_collections("a", "dust", true)), vec![1]);
        assert_eq!(
            ids(book.visible_collections("a", "copper", false)),
            Vec::<i32>::new()
        );
    }

    #[test]
    fn a_busca_com_dois_pontos_exige_o_espaco_de_nomes_e_acha_o_caminho_ou_o_nome() {
        let mut collection = Collection::new(vec![entry(1, "a", None)]);
        collection.names = vec!["enriched iron".to_owned()];
        collection.ids = vec![("mekanism".to_owned(), "enriched_iron".to_owned())];
        assert!(collection.matches_search("enriched"));
        assert!(!collection.matches_search("enriched_iron"));
        assert!(collection.matches_search("mekanism:enriched_iron"));
        assert!(collection.matches_search("mek : enriched iron"));
        assert!(!collection.matches_search("minecraft:enriched_iron"));
        assert!(collection.matches_search("mekanism:"));
        assert!(!collection.matches_search("mekanism:gold"));
    }

    #[test]
    fn a_paginacao_e_de_vinte_e_volta_a_zero_quando_a_pagina_some() {
        let mut page = Page::default();
        page.update_collections((0..45).collect(), false, false);
        assert_eq!((page.total_pages, page.current_page), (3, 0));
        assert!(page.forward_visible() && !page.back_visible());
        assert_eq!(page.shown().len(), 20);
        assert!(page.forward() && page.forward());
        assert_eq!(
            (page.current_page, page.shown().to_vec()),
            (2, (40..45).collect::<Vec<_>>())
        );
        assert!(!page.forward_visible() && page.back_visible());
        assert!(!page.forward());
        // A lista encolhe: a página 2 deixa de existir e volta a 0, sem pedido de reset.
        page.update_collections((0..30).collect(), false, false);
        assert_eq!((page.total_pages, page.current_page), (2, 0));
        // Ainda dentro do total, a página fica, a não ser que peçam o reset (a troca de aba).
        page.forward();
        page.update_collections((0..30).collect(), false, false);
        assert_eq!(page.current_page, 1);
        page.update_collections((0..30).collect(), true, false);
        assert_eq!(page.current_page, 0);
        // Vinte coleções cabem numa página só, sem setas nem o texto `Page`.
        page.update_collections((0..20).collect(), false, false);
        assert_eq!(page.total_pages, 1);
        assert!(!page.forward_visible() && !page.back_visible() && page.page_label().is_none());
        page.update_collections(Vec::new(), false, false);
        assert_eq!(page.total_pages, 0);
        assert!(page.shown().is_empty());
        page.update_collections((0..21).collect(), false, false);
        assert_eq!(page.page_label(), Some((1, 2)));
    }

    #[test]
    fn o_botao_alterna_a_receita_a_cada_30_de_tempo_e_o_item_a_cada_volta() {
        assert_eq!(slot_select_index(29.9), 0);
        assert_eq!(slot_select_index(30.0), 1);
        assert_eq!(slot_select_index(95.0), 3);
        assert_eq!(current_recipe_index(7, 3), 1);
        assert_eq!(display_stack_index(7, 3), (1, 2));
        assert_eq!(display_stack_index(2, 3), (2, 0));
    }

    #[test]
    fn a_aba_so_aparece_com_receita_escolhida_e_pisca_com_receita_nova() {
        let mut book = book(&[entry(1, "a", None), entry(2, "b", None)]);
        assert!(!book.tab_visible("a"));
        let first = book.tab("a")[0];
        book.collection_mut(first).select(|_| true, |_| true);
        assert!(book.tab_visible("a") && !book.tab_visible("b"));
        assert!(!book.tab_highlighted("a", false));
        book.add_highlight(1);
        assert!(book.tab_highlighted("a", false) && book.tab_highlighted("a", true));
        book.collection_mut(first).select(|_| true, |_| false);
        assert!(book.tab_highlighted("a", false) && !book.tab_highlighted("a", true));
        book.remove_highlight(1);
        assert!(!book.tab_highlighted("a", false));
    }

    #[test]
    fn the_point_test_is_open_on_the_right_and_at_the_bottom() {
        let rect = [10, 20, 5, 5];
        assert!(contains(rect, 10.0, 20.0) && contains(rect, 14.9, 24.9));
        assert!(
            !contains(rect, 15.0, 22.0)
                && !contains(rect, 12.0, 25.0)
                && !contains(rect, 9.9, 22.0)
        );
    }
}
