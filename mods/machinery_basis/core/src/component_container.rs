//! Porte de `ComponentBackedResourceContainer` e `ComponentBackedResourceHandler`: o tanque que mora num componente do item (o
//! `mekanism:fluids` do Fluid Tank, o `mekanism:chemicals` do Chemical Tank), visto pela capability de recurso do item.
//!
//! O contêiner não olha o tamanho da pilha: quem divide o pedido pela pilha e multiplica a resposta de volta é o handler. O limite de
//! taxa é o `createManualBypassRateLimit` (só o `MANUAL` escapa) e nasce com o contêiner, que nasce com o handler: cada capability
//! pedida tem a sua conta.
//!
//! A lib não conhece nenhum componente: quem usa diz como ler e gravar o conteúdo no item ([`AttachedCodec`]) e o que o item é
//! ([`ComponentTankSpec`], lido do item na hora em que o contêiner nasce, como o `create(attachedAccess, tankIndex)` lê o tier).
use crate::containers::{AutomationType, ItemResource};
use crate::item_access::{ItemAccess, ItemResourceHandler};
use crate::rate_limit::RateLimit;
use crate::resource_tank::StackContents;
use crate::transaction::Transaction;
use std::cell::{Cell, OnceCell};
use std::rc::Rc;

/// O que o `ResourceContainerType` faz com o item: ler o contêiner do componente e devolver o item com o contêiner novo.
pub trait AttachedCodec<R> {
    /// `getContents(getAttached())`: o conteúdo do contêiner 0 do componente, ou o vazio se o item não tem o componente.
    fn contents(&self, item: &ItemResource) -> StackContents<R>;
    /// `attachedTo.with(componentType, attached.with(0, value))`, criando o componente se o item não o tem
    /// (`createNewAttachment`). `None` quando o item não sabe guardar esse tipo de contêiner.
    fn with_contents(
        &self,
        item: &ItemResource,
        contents: &StackContents<R>,
    ) -> Option<ItemResource>;
}

/// `BiPredicate<RESOURCE, AutomationType>`.
pub type ComponentRule<R> = Rc<dyn Fn(&R, AutomationType) -> bool>;

/// O que o construtor do contêiner recebe.
#[derive(Clone)]
pub struct ComponentTankSpec<R> {
    /// `capacity` e `rate` (os do tier).
    pub capacity: u64,
    pub rate: i32,
    /// `isCreative` (`ComponentBackedFluidTankFluidTank`, `ComponentBackedChemicalTankTank`): aceita e entrega sem mudar o conteúdo.
    pub creative: bool,
    pub can_extract: ComponentRule<R>,
    pub can_insert: ComponentRule<R>,
    /// `isValid`: o `validator` e, no químico, o `ChemicalAttributeValidator`.
    pub valid: Rc<dyn Fn(&R) -> bool>,
}

impl<R: 'static> ComponentTankSpec<R> {
    /// O tanque do item de um bloco de tanque: tudo entra e tudo sai (`alwaysTrueBi`, `alwaysTrue`).
    pub fn open(capacity: u64, rate: i32, creative: bool) -> Self {
        Self {
            capacity,
            rate,
            creative,
            can_extract: Rc::new(|_, _| true),
            can_insert: Rc::new(|_, _| true),
            valid: Rc::new(|_| true),
        }
    }
}

/// O que o item é, lido quando o contêiner nasce (`IContainerCreator.create(attachedAccess, containerIndex)`).
pub type ComponentTankSpecOf<R> = Rc<dyn Fn(&ItemResource) -> ComponentTankSpec<R>>;

/// `ComponentBackedResourceContainer`, com os dois limites de taxa dele.
struct Container<R> {
    spec: ComponentTankSpec<R>,
    insertion: RateLimit,
    extraction: RateLimit,
}

/// `ComponentBackedResourceHandler` de um contêiner só, com `validateItemType` ligado (como todo handler de recurso que o Mekanism
/// expõe num item).
pub struct ComponentBackedHandler<'a, R> {
    access: &'a dyn ItemAccess,
    codec: Rc<dyn AttachedCodec<R>>,
    spec_of: ComponentTankSpecOf<R>,
    game_time: Rc<Cell<u64>>,
    /// `initialItemType`.
    initial_item: Option<Rc<str>>,
    /// Os contêineres nascem na primeira vez em que são usados (`initializeContainer`).
    container: OnceCell<Container<R>>,
    /// O `AutomationType` dos métodos sem ele: `EXTERNAL` (`defaultAutomationType`), ou `MANUAL` por dentro de `AutomatedResourceHandler.manual`.
    automation: AutomationType,
}

fn saturated(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl<'a, R: Clone + PartialEq> ComponentBackedHandler<'a, R> {
    pub fn new(
        access: &'a dyn ItemAccess,
        codec: Rc<dyn AttachedCodec<R>>,
        spec_of: ComponentTankSpecOf<R>,
        game_time: Rc<Cell<u64>>,
    ) -> Self {
        let initial_item = access.resource().map(|item| item.id);
        Self {
            access,
            codec,
            spec_of,
            game_time,
            initial_item,
            container: OnceCell::new(),
            automation: AutomationType::External,
        }
    }

    /// `AutomatedResourceHandler.manual(handler)`: o mesmo handler agindo como `MANUAL`, que passa pelo limite de taxa do contêiner (`createManualBypassRateLimit`): é como o
    /// item do tanque serve de balde mesmo com a taxa do tier abaixo de um balde.
    pub fn manual(mut self) -> Self {
        self.automation = AutomationType::Manual;
        self
    }

    /// `isAccessInvalid`: o acesso ficou sem item, ou o item dele deixou de ser o de quando o handler nasceu.
    fn is_access_invalid(&self) -> bool {
        if self.access.amount() == 0 {
            return true;
        }
        match (&self.initial_item, self.access.resource()) {
            (Some(initial), Some(held)) => *initial != held.id,
            _ => true,
        }
    }

    /// `getContainer(0)`.
    fn container(&self) -> &Container<R> {
        self.container.get_or_init(|| {
            let item = self
                .access
                .resource()
                .expect("o contêiner de um item nasce com o item no acesso");
            let spec = (self.spec_of)(&item);
            Container {
                insertion: RateLimit::manual_bypass(self.game_time.clone(), spec.rate),
                extraction: RateLimit::manual_bypass(self.game_time.clone(), spec.rate),
                spec,
            }
        })
    }

    /// `asStack()`: o conteúdo, sem multiplicar pela pilha.
    fn contents(&self) -> StackContents<R> {
        match self.access.resource() {
            Some(item) => self.codec.contents(&item),
            None => StackContents::default(),
        }
    }

    /// `ComponentBackedContainer.setContents(attached, value, transaction, false)`.
    fn set_contents(&self, resource: &R, amount: u64, transaction: &Transaction) -> bool {
        let Some(attached_to) = self.access.resource() else {
            return false;
        };
        let value = StackContents::new(Some(resource.clone()), amount);
        let Some(updated) = self.codec.with_contents(&attached_to, &value) else {
            return false;
        };
        // `ItemAccessUtils.exchange`: a pilha inteira do acesso troca de item.
        self.access
            .exchange(&updated, self.access.amount(), transaction)
            != 0
    }

    /// `capacityAsLong(resource)`: zero para o que o contêiner nunca aceita.
    fn capacity_of(&self, resource: Option<&R>) -> u64 {
        let container = self.container();
        if resource.is_some_and(|resource| !(container.spec.valid)(resource)) {
            return 0;
        }
        container.spec.capacity
    }

    /// `ComponentBackedResourceContainer.insert(resource, amount, transaction, automationType)`.
    fn container_insert(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 {
            return 0;
        }
        let capacity = self.capacity_of(Some(resource));
        if capacity == 0 {
            return 0;
        }
        let current = self.contents();
        if self.container().spec.creative {
            // Aceita sem mudar o conteúdo.
            let simulation = Transaction::open(Some(transaction));
            return self.insert_into(
                &current,
                capacity,
                resource,
                amount,
                &simulation,
                automation,
            );
        }
        self.insert_into(
            &current,
            capacity,
            resource,
            amount,
            transaction,
            automation,
        )
    }

    fn insert_into(
        &self,
        current: &StackContents<R>,
        capacity: u64,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if current
            .resource
            .as_ref()
            .is_some_and(|held| held != resource)
        {
            return 0;
        }
        let container = self.container();
        let needed = saturated(capacity.saturating_sub(current.amount))
            .min(container.insertion.remaining(automation));
        if needed <= 0 || !(container.spec.can_insert)(resource, automation) {
            return 0;
        }
        let to_add = amount.min(needed);
        if self.set_contents(resource, current.amount + to_add as u64, transaction) {
            container.insertion.consume(to_add, automation, transaction);
            return to_add;
        }
        0
    }

    /// `ComponentBackedResourceContainer.extract(resource, amount, transaction, automationType)`.
    fn container_extract(
        &self,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        if amount == 0 {
            return 0;
        }
        let current = self.contents();
        if self.container().spec.creative {
            let simulation = Transaction::open(Some(transaction));
            return self.extract_from(&current, resource, amount, &simulation, automation);
        }
        self.extract_from(&current, resource, amount, transaction, automation)
    }

    fn extract_from(
        &self,
        current: &StackContents<R>,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
        automation: AutomationType,
    ) -> i32 {
        let container = self.container();
        if current.resource.as_ref() != Some(resource)
            || !(container.spec.can_extract)(resource, automation)
        {
            return 0;
        }
        let to_remove = amount
            .min(saturated(current.amount))
            .min(container.extraction.remaining(automation));
        if to_remove > 0
            && self.set_contents(resource, current.amount - to_remove as u64, transaction)
        {
            container
                .extraction
                .consume(to_remove, automation, transaction);
            return to_remove;
        }
        0
    }

    /// `getPerItem`.
    fn per_item(&self, amount: i32) -> i32 {
        if self.is_access_invalid() {
            0
        } else {
            amount / self.access.amount()
        }
    }
}

/// O que a capability do item entrega: os métodos sem `AutomationType` valem como `EXTERNAL` (`defaultAutomationType`).
impl<R: Clone + PartialEq> ItemResourceHandler<R> for ComponentBackedHandler<'_, R> {
    fn size(&self) -> usize {
        1
    }

    fn resource(&self, _index: usize) -> Option<R> {
        if self.is_access_invalid() {
            return None;
        }
        self.contents().resource
    }

    fn amount_as_int(&self, _index: usize) -> i32 {
        if self.is_access_invalid() {
            return 0;
        }
        // `MathUtils.multiplyClamped(attachedAccess.getAmount(), amount)`.
        saturated(
            u64::try_from(self.access.amount())
                .unwrap_or(0)
                .saturating_mul(self.contents().amount),
        )
    }

    fn insert(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        let per_item = self.per_item(amount);
        if per_item == 0 {
            return 0;
        }
        let count = self.access.amount();
        count * self.container_insert(resource, per_item, transaction, self.automation)
    }

    fn extract(&self, resource: &R, amount: i32, transaction: &Transaction) -> i32 {
        self.extract_at(0, resource, amount, transaction)
    }

    fn extract_at(
        &self,
        _index: usize,
        resource: &R,
        amount: i32,
        transaction: &Transaction,
    ) -> i32 {
        let per_item = self.per_item(amount);
        if per_item == 0 {
            return 0;
        }
        let count = self.access.amount();
        count * self.container_extract(resource, per_item, transaction, self.automation)
    }

    fn is_full(&self) -> bool {
        // `ResourceHandlerUtil.isFull`: `getAmountAsLong(0) >= getCapacityAsLong(0, getResource(0))`, os dois multiplicados pela pilha.
        if self.is_access_invalid() {
            // Sem item o handler diz 0 de 0.
            return true;
        }
        let count = u64::try_from(self.access.amount()).unwrap_or(0);
        let contents = self.contents();
        count.saturating_mul(contents.amount)
            >= count.saturating_mul(self.capacity_of(contents.resource.as_ref()))
    }
}
