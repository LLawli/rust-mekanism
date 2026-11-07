//! Porte de `BasicChemicalTank` (sobre `BasicResourceContainer`, em `resource_tank`), de `ChemicalAttributeValidator`, do
//! handler de entrada e de saída de químico e de `ChemicalInventorySlot.fillTankOrConvert`. Como os outros contêineres da
//! base, o tanque é um objeto compartilhado, com diário: registra-se sozinho na transação que o muda e avisa quem escuta no
//! commit da raiz.
//!
//! A lib não conhece nenhum químico: quem usa dá o id, as regras de inserção, os atributos e as conversões como dados.
use crate::cached_recipe::ItemIngredient;
use crate::containers::{Access, AutomationType, InventorySlot, ItemResource, SlotContents};
use crate::recipe_cache::{
    ComponentSensitiveInputCache, InputIngredient, ShapedIngredient, SingleInputRecipe,
    SingleInputRecipeCache,
};
use crate::recipe_handlers::{ResourceOutput, TankInputHandler, TankOutputHandler};
use crate::resource_tank::{
    Ingredient, ResourceHandler, ResourceTank, Rule, StackContents, Validator,
    fill_container_from_slot,
};
use crate::transaction::{Callback, Transaction};
use std::collections::BTreeSet;
use std::rc::Rc;

/// Um químico, pelo id. Vazio é a ausência dele (`ChemicalResource.EMPTY`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChemicalResource {
    pub id: Rc<str>,
}

impl ChemicalResource {
    pub fn new(id: &str) -> Self {
        Self { id: Rc::from(id) }
    }
}

/// `LargeResourceStack<ChemicalResource>`: o que um tanque guarda. Vazio é sem recurso e com zero.
pub type TankContents = StackContents<ChemicalResource>;

/// `BiPredicate<ChemicalResource, AutomationType>`.
pub type ChemicalRule = Rule<ChemicalResource>;
/// `Predicate<ChemicalResource>`.
pub type ChemicalValidator = Validator<ChemicalResource>;

/// `BasicChemicalTank`.
///
/// O validador de atributos entra por [`ChemicalTank::create_with_attributes`]; sem ele (`create`, `open`), nenhum químico
/// tem atributo, e o tanque aceita o que o validador comum aceitar.
pub type ChemicalTank = ResourceTank<ChemicalResource>;

/// Um `IChemicalAttribute` de um químico: o tipo dele (a classe, no Java) e se precisa de validação (`needsValidation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemicalAttribute {
    pub kind: Rc<str>,
    pub needs_validation: bool,
}

impl ChemicalAttribute {
    pub fn new(kind: &str, needs_validation: bool) -> Self {
        Self {
            kind: Rc::from(kind),
            needs_validation,
        }
    }
}

/// Os atributos de um químico: `IMekanismDataMapTypes.chemicalAttributeTypes()` com `getData` de cada um. Vem dos dados do mod,
/// e `needs_validation` pode mudar com a configuração (a radioatividade só pede validação com a radiação ligada).
pub type ChemicalAttributes = Rc<dyn Fn(&ChemicalResource) -> Vec<ChemicalAttribute>>;

type AttributeCheck = Rc<dyn Fn(&ChemicalAttribute) -> bool>;
type ProcessCheck = Rc<dyn Fn(&ChemicalResource) -> bool>;

/// `ChemicalAttributeValidator`: decide, pelos atributos de um químico, se um tanque o aceita.
#[derive(Clone)]
pub struct ChemicalAttributeValidator {
    validate: AttributeCheck,
    /// O `process` sobrescrito (`ALWAYS_ALLOW`, o `StackedWasteBarrel`). Sem ele vale o padrão da interface: todos os
    /// atributos do químico têm de passar em `validate`.
    process: Option<ProcessCheck>,
}

impl ChemicalAttributeValidator {
    /// `DEFAULT`: recusa o atributo que pede validação.
    pub fn default_rule() -> Self {
        Self::custom(|attribute| !attribute.needs_validation)
    }

    /// `ALWAYS_ALLOW`: aceita tudo, sem nem olhar os atributos.
    pub fn always_allow() -> Self {
        Self {
            validate: Rc::new(|_| true),
            process: Some(Rc::new(|_| true)),
        }
    }

    /// `create(validAttributes...)`: aceita os tipos dados e o que não pede validação.
    pub fn create(valid: &[&str]) -> Self {
        Self::simple(valid, true)
    }

    /// `createStrict(validAttributes...)`: aceita só os tipos dados.
    pub fn create_strict(valid: &[&str]) -> Self {
        Self::simple(valid, false)
    }

    fn simple(valid: &[&str], allow_no_validation: bool) -> Self {
        let valid: BTreeSet<Rc<str>> = valid.iter().map(|kind| Rc::from(*kind)).collect();
        Self::custom(move |attribute| {
            (allow_no_validation && !attribute.needs_validation) || valid.contains(&attribute.kind)
        })
    }

    /// Um validador com `validate` próprio, e o `process` padrão.
    pub fn custom(validate: impl Fn(&ChemicalAttribute) -> bool + 'static) -> Self {
        Self {
            validate: Rc::new(validate),
            process: None,
        }
    }

    /// Um validador que sobrescreve também o `process`, como o do `StackedWasteBarrel`.
    pub fn custom_process(
        validate: impl Fn(&ChemicalAttribute) -> bool + 'static,
        process: impl Fn(&ChemicalResource) -> bool + 'static,
    ) -> Self {
        Self {
            validate: Rc::new(validate),
            process: Some(Rc::new(process)),
        }
    }

    /// `validate`.
    pub fn validate(&self, attribute: &ChemicalAttribute) -> bool {
        (self.validate)(attribute)
    }

    /// `process`: nenhum atributo do químico pode ser recusado.
    pub fn process(&self, resource: &ChemicalResource, attributes: &ChemicalAttributes) -> bool {
        if let Some(process) = &self.process {
            return process(resource);
        }
        attributes(resource)
            .iter()
            .all(|attribute| self.validate(attribute))
    }
}

impl ChemicalTank {
    /// `BasicChemicalTank.create(capacity, canExtract, canInsert, validator, attributeValidator, listener)`. O `isValid` do
    /// Java é `getAttributeValidator().process(chemical) && super.isValid(chemical)`, e `None` é o `DEFAULT`.
    pub fn create_with_attributes(
        capacity: u64,
        can_extract: ChemicalRule,
        can_insert: ChemicalRule,
        validator: ChemicalValidator,
        attribute_validator: Option<ChemicalAttributeValidator>,
        attributes: ChemicalAttributes,
        listener: Option<Callback>,
    ) -> Self {
        let attribute_validator =
            attribute_validator.unwrap_or_else(ChemicalAttributeValidator::default_rule);
        let validator: ChemicalValidator = Rc::new(move |resource| {
            attribute_validator.process(resource, &attributes) && validator(resource)
        });
        Self::create(capacity, can_extract, can_insert, validator, listener)
    }
}

/// Os predicados de `AutomationType` que o Mekanism passa aos tanques, para quem quer o padrão sem escrever a closure.
pub fn access_rule(access: Access) -> ChemicalRule {
    Rc::new(move |_, automation| access.allows(automation))
}

/// Um químico que casa com o ingrediente de uma receita.
pub type ChemicalIngredient = Ingredient<ChemicalResource>;

impl ChemicalIngredient {
    /// Um químico só, pelo id.
    pub fn chemical(id: &str) -> Self {
        Self::shaped(ShapedIngredient::of_type(id))
    }
}

/// `InputHelper.getInputHandler(IChemicalTank, RecipeError)`.
pub type ChemicalInputHandler = TankInputHandler<ChemicalResource>;

/// `ChemicalStackTemplate`: o que uma receita entrega a um tanque de químico.
pub type ChemicalOutput = ResourceOutput<ChemicalResource>;

/// `OutputHelper.getOutputHandler(IChemicalTank, RecipeError)`.
pub type ChemicalOutputHandler = TankOutputHandler<ChemicalResource>;

/// `Capabilities.CHEMICAL.item()`: o que um item expõe para encher um tanque de químico.
pub trait ChemicalItems {
    /// A capability do item que está no slot, ou `None` se ele não tem. É o `getCapability(asItemAccess())`.
    fn handler(&self, slot: &InventorySlot) -> Option<Rc<dyn ResourceHandler<ChemicalResource>>>;
}

/// Itens sem capability de químico: o que `fillTankFromSlot` acha em qualquer item do jogo que não guarda químico.
pub struct NoChemicalItems;

impl ChemicalItems for NoChemicalItems {
    fn handler(&self, _slot: &InventorySlot) -> Option<Rc<dyn ResourceHandler<ChemicalResource>>> {
        None
    }
}

/// `ChemicalInventorySlot.fillTankFromSlot(transaction)`: enche o tanque de um item com capability de químico. Devolve se
/// encheu.
pub fn fill_tank_from_slot(
    slot: &InventorySlot,
    tank: &ChemicalTank,
    items: &dyn ChemicalItems,
    transaction: Option<&Transaction>,
) -> bool {
    // Só um slot com item chega a pedir a capability, como o `!isEmpty()` do Java.
    let handler = if slot.is_empty() {
        None
    } else {
        items.handler(slot)
    };
    fill_container_from_slot(slot, handler.as_deref(), tank, transaction)
}

/// Uma receita de `chemical_conversion` (`ItemStackToChemicalRecipe`): um item que vira químico.
#[derive(Clone, Debug)]
pub struct ChemicalConversion {
    pub input: ItemIngredient,
    pub input_count: u32,
    pub output: ChemicalResource,
    pub amount: i32,
}

impl SingleInputRecipe<SlotContents> for ChemicalConversion {
    /// `ItemStackToChemicalRecipe.test(ItemStack)`: o ingrediente casa e a pilha tem o que ele pede.
    fn test(&self, input: &SlotContents) -> bool {
        input.resource.is_some() && self.input.test_type(input) && input.amount >= self.input_count
    }
}

/// O índice de `MekanismRecipeType.CHEMICAL_CONVERSION` (`InputRecipeCache.SingleItem`).
pub type ConversionCache = SingleInputRecipeCache<
    ChemicalConversion,
    ItemIngredient,
    SlotContents,
    ComponentSensitiveInputCache,
>;

impl ChemicalConversion {
    /// O índice das conversões que `source` devolve, como o `getRecipes(world)` do tipo: lido na primeira pergunta depois de um
    /// `clear`.
    pub fn cache(source: impl Fn() -> Vec<Rc<Self>> + 'static) -> Rc<ConversionCache> {
        Rc::new(SingleInputRecipeCache::new(
            source,
            |conversion: &Self| &conversion.input,
            ComponentSensitiveInputCache::new(),
        ))
    }
}

impl ChemicalConversion {
    /// O índice de uma lista que não muda.
    pub fn cache_of(conversions: Vec<Self>) -> Rc<ConversionCache> {
        let conversions: Vec<Rc<Self>> = conversions.into_iter().map(Rc::new).collect();
        Self::cache(move || conversions.clone())
    }
}

/// `ChemicalInventorySlot.canFillOrConvert` para quem não tem item com capability de químico: o item entra se a primeira
/// conversão do tipo dele (`findTypeBasedRecipe`, sem a quantidade) dá um químico que o tanque aceita e que combina com o que
/// ele guarda.
pub fn can_fill_or_convert(
    tank: &ChemicalTank,
    conversions: &ConversionCache,
    item: &ItemResource,
) -> bool {
    can_fill_or_convert_with(conversions, item, tank.resource().as_ref(), |chemical| {
        tank.is_valid_for_insertion(chemical, AutomationType::Manual)
    })
}

/// O mesmo para quem não tem o tanque na mão (o menu): `held` é o que o tanque guarda e `valid_for_insertion` o
/// `isValidForInsertion(.., MANUAL)` dele. `Manual` passa por cima do limite de taxa de quem insere.
pub fn can_fill_or_convert_with(
    conversions: &ConversionCache,
    item: &ItemResource,
    held: Option<&ChemicalResource>,
    valid_for_insertion: impl Fn(&ChemicalResource) -> bool,
) -> bool {
    let Some(conversion) = conversions.find_type_based_recipe(item) else {
        // Sem receita, não entra.
        return false;
    };
    // `simulateCanInsert`: o químico é válido para o tanque, e o tanque está vazio ou já guarda esse.
    valid_for_insertion(&conversion.output) && held.is_none_or(|held| *held == conversion.output)
}

/// `ChemicalInventorySlot.fillTankOrConvert` para quem não tem item com capability de químico: vai direto à conversão.
pub fn fill_tank_or_convert(
    slot: &InventorySlot,
    tank: &ChemicalTank,
    conversions: &ConversionCache,
    transaction: Option<&Transaction>,
) {
    fill_tank_or_convert_with(slot, tank, conversions, &NoChemicalItems, transaction);
}

/// `ChemicalInventorySlot.fillTankOrConvert`: primeiro enche o tanque do item do slot, se ele tem a capability de químico, e
/// só se isso falhar o item vira químico no tanque, se o tanque aceita tudo o que a conversão dá.
pub fn fill_tank_or_convert_with(
    slot: &InventorySlot,
    tank: &ChemicalTank,
    conversions: &ConversionCache,
    items: &dyn ChemicalItems,
    transaction: Option<&Transaction>,
) {
    if fill_tank_from_slot(slot, tank, items, transaction) {
        return;
    }
    let contents = slot.contents();
    let Some(resource) = contents.resource.clone() else {
        return;
    };
    // `findFirstRecipe`: a primeira conversão que o índice acha para o que o slot tem, quantidade incluída. O
    // `getMatchingInstance` que vem depois só devolve vazio onde o `test` já teria falhado.
    let Some(conversion) = conversions.find_first_recipe(&contents) else {
        return;
    };
    let sub_transaction = Transaction::open(transaction);
    let needed = conversion.input_count;
    let produced = conversion.amount;
    // Tira do slot o que a receita pede e põe o que ela dá no tanque. `Manual` passa por cima do limite de taxa de quem
    // insere, que aqui não existe, mas o `canInsert` do tanque ainda vale.
    if slot.extract(
        &resource,
        needed,
        &sub_transaction,
        AutomationType::Internal,
    ) == needed
        && tank.insert(
            &conversion.output,
            produced,
            &sub_transaction,
            AutomationType::Manual,
        ) == produced
    {
        sub_transaction.commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::ItemResource;
    use crate::operation_tracker::RecipeError;
    use std::cell::Cell;

    fn redstone() -> ChemicalResource {
        ChemicalResource::new("redstone")
    }

    fn counting() -> (Callback, Rc<Cell<u32>>) {
        let count = Rc::new(Cell::new(0));
        let inner = count.clone();
        (Rc::new(move || inner.set(inner.get() + 1)), count)
    }

    #[test]
    fn a_tank_holds_one_chemical_up_to_its_capacity() {
        let tank = ChemicalTank::open(1_000, None);
        let root = Transaction::open_root();
        assert_eq!(
            tank.insert(&redstone(), 600, &root, AutomationType::Internal),
            600
        );
        assert_eq!(
            tank.insert(&redstone(), 600, &root, AutomationType::Internal),
            400
        );
        let other = ChemicalResource::new("carbon");
        assert_eq!(tank.insert(&other, 1, &root, AutomationType::Internal), 0);
    }

    #[test]
    fn a_rolled_back_change_says_nothing() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        let root = Transaction::open_root();
        tank.insert(&redstone(), 5, &root, AutomationType::Internal);
        drop(root);
        assert!(tank.is_empty());
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn a_commit_says_it_once_at_the_root() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        let root = Transaction::open_root();
        let nested = Transaction::open(Some(&root));
        tank.insert(&redstone(), 5, &nested, AutomationType::Internal);
        nested.commit();
        assert_eq!(count.get(), 0);
        root.commit();
        assert_eq!(count.get(), 1);
        assert_eq!(tank.amount(), 5);
    }

    #[test]
    fn setting_outside_a_transaction_says_it_at_once() {
        let (listener, count) = counting();
        let tank = ChemicalTank::open(1_000, Some(listener));
        tank.set_contents(Some(redstone()), 15, None);
        assert_eq!(count.get(), 1);
        tank.set_contents(Some(redstone()), 15, None);
        assert_eq!(count.get(), 1, "o mesmo conteúdo não avisa");
        tank.set_contents(None, 0, None);
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn what_the_validator_refuses_has_no_room() {
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::Any),
            access_rule(Access::Any),
            Rc::new(|resource| &*resource.id == "redstone"),
            None,
        );
        let root = Transaction::open_root();
        let other = ChemicalResource::new("carbon");
        assert_eq!(tank.capacity(&other), 0);
        assert_eq!(tank.insert(&other, 5, &root, AutomationType::Internal), 0);
        assert_eq!(
            tank.insert(&redstone(), 5, &root, AutomationType::Internal),
            5
        );
    }

    #[test]
    fn extraction_follows_the_rule_for_who_asks() {
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::NotExternal),
            access_rule(Access::Any),
            Rc::new(|_| true),
            None,
        );
        tank.set_contents(Some(redstone()), 50, None);
        let root = Transaction::open_root();
        assert_eq!(
            tank.extract(&redstone(), 10, &root, AutomationType::External),
            0
        );
        assert_eq!(
            tank.extract(&redstone(), 10, &root, AutomationType::Internal),
            10
        );
        assert_eq!(
            tank.extract(&redstone(), 100, &root, AutomationType::Manual),
            40
        );
        assert!(tank.is_empty());
    }

    fn conversions(conversions: Vec<ChemicalConversion>) -> Rc<ConversionCache> {
        let conversions: Vec<Rc<ChemicalConversion>> =
            conversions.into_iter().map(Rc::new).collect();
        ChemicalConversion::cache(move || conversions.clone())
    }

    fn dust_to_redstone() -> Rc<ConversionCache> {
        conversions(vec![ChemicalConversion {
            input: ItemIngredient::item("redstone"),
            input_count: 1,
            output: redstone(),
            amount: 10,
        }])
    }

    fn dust_slot(amount: u32) -> InventorySlot {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(ItemResource::new("redstone", 64)), amount, None);
        slot
    }

    #[test]
    fn an_item_becomes_the_chemical_one_at_a_time() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::open(1_000, None);
        fill_tank_or_convert(&slot, &tank, &dust_to_redstone(), None);
        assert_eq!((slot.amount(), tank.amount()), (2, 10));
        fill_tank_or_convert(&slot, &tank, &dust_to_redstone(), None);
        assert_eq!((slot.amount(), tank.amount()), (1, 20));
    }

    #[test]
    fn a_conversion_that_does_not_fit_in_the_tank_keeps_the_item() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::open(15, None);
        tank.set_contents(Some(redstone()), 10, None);
        fill_tank_or_convert(&slot, &tank, &dust_to_redstone(), None);
        assert_eq!((slot.amount(), tank.amount()), (3, 10));
    }

    #[test]
    fn a_conversion_the_tank_refuses_keeps_the_item() {
        let slot = dust_slot(3);
        let tank = ChemicalTank::create(
            1_000,
            access_rule(Access::Any),
            Rc::new(|_, _| false),
            Rc::new(|_| true),
            None,
        );
        fill_tank_or_convert(&slot, &tank, &dust_to_redstone(), None);
        assert_eq!((slot.amount(), tank.amount()), (3, 0));
    }

    fn radioactive(needs_validation: bool) -> ChemicalAttribute {
        ChemicalAttribute::new("radioactivity", needs_validation)
    }

    fn fuel() -> ChemicalAttribute {
        ChemicalAttribute::new("fuel", false)
    }

    /// Quais atributos cada químico tem, como o `getData` dos mapas de dados.
    fn attributes(radiation_enabled: bool) -> ChemicalAttributes {
        Rc::new(move |chemical: &ChemicalResource| match &*chemical.id {
            "plutonium" => vec![radioactive(radiation_enabled)],
            "ethene" => vec![fuel()],
            "polonium" => vec![fuel(), radioactive(radiation_enabled)],
            _ => Vec::new(),
        })
    }

    fn accepted(validator: &ChemicalAttributeValidator, id: &str, radiation: bool) -> bool {
        validator.process(&ChemicalResource::new(id), &attributes(radiation))
    }

    #[test]
    fn the_default_validator_refuses_what_needs_validation() {
        let default = ChemicalAttributeValidator::default_rule();
        assert!(accepted(&default, "hydrogen", true));
        assert!(accepted(&default, "ethene", true));
        assert!(!accepted(&default, "plutonium", true));
        assert!(
            !accepted(&default, "polonium", true),
            "um atributo recusado basta"
        );
        // A radiação desligada tira a validação: o atributo existe, mas não pede nada.
        assert!(accepted(&default, "plutonium", false));
    }

    #[test]
    fn always_allow_does_not_even_look_at_the_attributes() {
        let looked = Rc::new(std::cell::Cell::new(0));
        let counter = looked.clone();
        let source: ChemicalAttributes = Rc::new(move |_| {
            counter.set(counter.get() + 1);
            vec![radioactive(true)]
        });
        let always = ChemicalAttributeValidator::always_allow();
        assert!(always.process(&ChemicalResource::new("plutonium"), &source));
        assert_eq!(looked.get(), 0);
        assert!(always.validate(&radioactive(true)));
    }

    #[test]
    fn the_listed_kinds_pass_and_the_non_strict_one_also_passes_what_needs_nothing() {
        let loose = ChemicalAttributeValidator::create(&["radioactivity"]);
        assert!(accepted(&loose, "plutonium", true));
        assert!(accepted(&loose, "ethene", true), "não pede validação");
        let strict = ChemicalAttributeValidator::create_strict(&["radioactivity"]);
        assert!(accepted(&strict, "plutonium", true));
        assert!(!accepted(&strict, "ethene", true), "só os tipos listados");
        assert!(
            accepted(&strict, "hydrogen", true),
            "sem atributo não há o que recusar"
        );
    }

    #[test]
    fn a_validator_can_replace_the_process_as_the_waste_barrel_does() {
        // O `StackedWasteBarrel` só aceita o radioativo: `process` ignora os atributos que a fonte dá.
        let barrel = ChemicalAttributeValidator::custom_process(
            |attribute| &*attribute.kind == "radioactivity",
            |chemical| &*chemical.id == "plutonium",
        );
        assert!(accepted(&barrel, "plutonium", false));
        assert!(!accepted(&barrel, "hydrogen", false));
        assert!(barrel.validate(&radioactive(false)));
        assert!(!barrel.validate(&fuel()));
    }

    fn tank_with(
        attribute_validator: Option<ChemicalAttributeValidator>,
        radiation: bool,
    ) -> ChemicalTank {
        ChemicalTank::create_with_attributes(
            1_000,
            access_rule(Access::Any),
            access_rule(Access::Any),
            Rc::new(|chemical| &*chemical.id != "forbidden"),
            attribute_validator,
            attributes(radiation),
            None,
        )
    }

    #[test]
    fn a_tank_refuses_what_the_attribute_validator_refuses_before_its_own_validator() {
        let tank = tank_with(None, true);
        let root = Transaction::open_root();
        let plutonium = ChemicalResource::new("plutonium");
        assert!(!tank.is_valid(&plutonium));
        assert_eq!(tank.capacity(&plutonium), 0);
        assert_eq!(
            tank.insert(&plutonium, 5, &root, AutomationType::Internal),
            0
        );
        let hydrogen = ChemicalResource::new("hydrogen");
        assert!(tank.is_valid(&hydrogen));
        assert_eq!(
            tank.insert(&hydrogen, 5, &root, AutomationType::Internal),
            5
        );
        // O validador comum do tanque segue valendo junto.
        assert!(!tank.is_valid(&ChemicalResource::new("forbidden")));
    }

    #[test]
    fn a_tank_with_always_allow_takes_the_radioactive_one_and_one_with_radiation_off_too() {
        let plutonium = ChemicalResource::new("plutonium");
        let creative = tank_with(Some(ChemicalAttributeValidator::always_allow()), true);
        assert!(creative.is_valid(&plutonium));
        assert!(!creative.is_valid(&ChemicalResource::new("forbidden")));
        assert!(tank_with(None, false).is_valid(&plutonium));
    }

    /// Um item com tanques de químico: o `ResourceHandler` que a capability dá.
    struct Canister {
        stored: std::cell::RefCell<Vec<(ChemicalResource, i32)>>,
        journal: Rc<crate::transaction::JournalCell<Taken>>,
    }

    struct Taken(i32);

    impl crate::transaction::Journaled for Taken {
        type Snapshot = i32;

        fn revert_to_snapshot(&mut self, snapshot: i32) {
            self.0 = snapshot;
        }

        fn on_root_commit(&mut self, _: i32) -> Vec<Callback> {
            Vec::new()
        }
    }

    impl ResourceHandler<ChemicalResource> for Canister {
        fn size(&self) -> usize {
            self.stored.borrow().len()
        }

        fn resource(&self, index: usize) -> Option<ChemicalResource> {
            Some(self.stored.borrow()[index].0.clone())
        }

        fn extract(
            &self,
            resource: &ChemicalResource,
            amount: i32,
            transaction: &Transaction,
        ) -> i32 {
            let available: i32 = self
                .stored
                .borrow()
                .iter()
                .filter(|(held, _)| held == resource)
                .map(|(_, held)| *held)
                .sum();
            let mut state = self.journal.borrow_mut();
            let to_take = amount.min(available - state.0);
            if to_take > 0 {
                self.journal.update_snapshots(transaction, || state.0);
                state.0 += to_take;
            }
            to_take.max(0)
        }
    }

    struct Canisters(Rc<Canister>);

    impl ChemicalItems for Canisters {
        fn handler(
            &self,
            slot: &InventorySlot,
        ) -> Option<Rc<dyn ResourceHandler<ChemicalResource>>> {
            let canister: Rc<dyn ResourceHandler<ChemicalResource>> = self.0.clone();
            (slot.resource()?.id.as_ref() == "canister").then_some(canister)
        }
    }

    fn canister(stored: Vec<(ChemicalResource, i32)>) -> Canisters {
        Canisters(Rc::new(Canister {
            stored: std::cell::RefCell::new(stored),
            journal: crate::transaction::JournalCell::new(Taken(0)),
        }))
    }

    fn canister_slot() -> InventorySlot {
        let slot = InventorySlot::new(None);
        slot.set_contents(Some(ItemResource::new("canister", 1)), 1, None);
        slot
    }

    #[test]
    fn an_item_with_the_capability_fills_the_tank_with_what_it_holds_and_the_tank_accepts() {
        let items = canister(vec![
            (ChemicalResource::new("forbidden"), 80),
            (redstone(), 120),
        ]);
        let tank = tank_with(None, true);
        let slot = canister_slot();
        assert!(fill_tank_from_slot(&slot, &tank, &items, None));
        assert_eq!((tank.resource(), tank.amount()), (Some(redstone()), 120));
        assert_eq!(items.0.journal.borrow().0, 120);
    }

    #[test]
    fn the_tank_refusing_the_attribute_leaves_the_item_alone() {
        let items = canister(vec![(ChemicalResource::new("plutonium"), 120)]);
        let tank = tank_with(None, true);
        assert!(!fill_tank_from_slot(&canister_slot(), &tank, &items, None));
        assert!(tank.is_empty());
        assert_eq!(items.0.journal.borrow().0, 0);
    }

    #[test]
    fn filling_from_the_item_comes_before_the_conversion_and_ends_the_tick_for_that_slot() {
        // Um item que é as duas coisas: tem a capability e uma conversão. A capability ganha, e o item não é consumido.
        let items = canister(vec![(redstone(), 30)]);
        let slot = canister_slot();
        let tank = ChemicalTank::open(1_000, None);
        let conversion = ChemicalConversion {
            input: ItemIngredient::item("canister"),
            input_count: 1,
            output: redstone(),
            amount: 500,
        };
        let cache = conversions(vec![conversion]);
        fill_tank_or_convert_with(&slot, &tank, &cache, &items, None);
        assert_eq!(
            (tank.resource(), tank.amount(), slot.amount()),
            (Some(redstone()), 30, 1)
        );
        // Sem a capability (ou com o item esvaziado), cai na conversão.
        fill_tank_or_convert_with(
            &slot,
            &ChemicalTank::open(1_000, None),
            &cache,
            &NoChemicalItems,
            None,
        );
        assert_eq!(slot.amount(), 0);
    }

    #[test]
    fn the_output_handler_counts_the_operations_that_fit_in_the_tank() {
        use crate::operation_tracker::{OperationTracker, RecipeErrors};
        let tank = ChemicalTank::open(100, None);
        tank.set_contents(Some(redstone()), 90, None);
        let handler =
            ChemicalOutputHandler::new(tank.clone(), RecipeError::NOT_ENOUGH_OUTPUT_SPACE);
        let output = ChemicalOutput {
            resource: redstone(),
            amount: 3,
        };
        let root = Transaction::open_root();
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, true, 50);
        crate::recipe_handlers::OutputHandler::calculate_operations_can_support(
            &handler,
            &mut tracker,
            &root,
            &output,
        );
        assert_eq!(tracker.current_max(), 3, "10 de espaço, 3 por operação");
        assert!(crate::recipe_handlers::OutputHandler::handle_output(
            &handler, &output, 3, &root
        ));
        assert_eq!(tank.amount(), 99);
        assert!(
            !crate::recipe_handlers::OutputHandler::handle_output(&handler, &output, 1, &root),
            "sobra 1"
        );
        assert!(
            crate::recipe_handlers::OutputHandler::handle_output(&handler, &output, 0, &root),
            "nenhuma operação sempre cabe"
        );
    }

    #[test]
    fn a_full_tank_has_no_space_and_a_tank_that_refuses_the_output_does_not_produce_it() {
        use crate::operation_tracker::{OperationTracker, RecipeErrors};
        let output = ChemicalOutput {
            resource: redstone(),
            amount: 3,
        };
        let root = Transaction::open_root();
        let full = ChemicalTank::open(100, None);
        full.set_contents(Some(redstone()), 100, None);
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, true, 50);
        crate::recipe_handlers::OutputHandler::calculate_operations_can_support(
            &ChemicalOutputHandler::new(full, RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
            &mut tracker,
            &root,
            &output,
        );
        assert_eq!(tracker.current_max(), 0);
        assert!(
            tracker
                .errors()
                .contains(RecipeError::NOT_ENOUGH_OUTPUT_SPACE)
        );
        let refusing = ChemicalTank::create(
            100,
            access_rule(Access::Any),
            access_rule(Access::Any),
            Rc::new(|_| false),
            None,
        );
        let mut tracker = OperationTracker::new(RecipeErrors::EMPTY, true, 50);
        crate::recipe_handlers::OutputHandler::calculate_operations_can_support(
            &ChemicalOutputHandler::new(refusing, RecipeError::NOT_ENOUGH_OUTPUT_SPACE),
            &mut tracker,
            &root,
            &output,
        );
        assert!(
            tracker
                .errors()
                .contains(RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT)
        );
        assert!(
            !tracker
                .errors()
                .contains(RecipeError::NOT_ENOUGH_OUTPUT_SPACE)
        );
    }
}
