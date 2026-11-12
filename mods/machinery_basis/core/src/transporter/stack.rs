//! `TransporterStack`: a pilha que anda num transportador logístico (o item, a cor, o progresso, a origem, a casa, o tipo e o caminho), o
//! NBT dela e o que ela decide sozinha (o lado, o próximo e o anterior, se pode entrar num transportador). Quem recalcula o caminho é a
//! [`StackRef`], porque o `TransporterManager` guarda a mesma pilha (por identidade) enquanto ela está em voo.
use super::nbt::{Compound, Nbt};
use super::pathfinder::{
    PathContext, RoundRobinEjector, get_idle_path, get_new_base_path, get_new_rr_path,
};
use super::pos::{NO_POS, side_difference};
use super::world::{InventoryNetworkView, Transporter};
use crate::containers::ItemResource;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::side_config::ENUM_COLOR_NAMES;
use crate::transaction::Transaction;
use crate::transit::{TransitRequest, TransitResponse};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

/// As chaves de NBT de `SerializationConstants`.
pub mod keys {
    pub const COLOR: &str = "color";
    pub const PROGRESS: &str = "progress";
    pub const ORIGINAL_LOCATION: &str = "original_location";
    pub const PATH_TYPE: &str = "path_type";
    pub const IDLE_DIR: &str = "idle_dir";
    pub const HOME_LOCATION: &str = "home_location";
    pub const ITEM: &str = "item";
    pub const TYPE: &str = "type";
    pub const AMOUNT: &str = "amount";
    pub const ID: &str = "id";
    pub const NEXT: &str = "next";
    pub const PREVIOUS: &str = "previous";
}

/// `TransporterStack.Path`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Dest,
    Home,
    None,
}

impl Path {
    /// `BY_ID` (`ByIdMap.continuous` com `OutOfBoundsStrategy.WRAP`).
    pub fn by_id(id: i32) -> Path {
        [Path::Dest, Path::Home, Path::None][id.rem_euclid(3) as usize]
    }

    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// `getSerializedName`.
    pub fn serialized_name(self) -> &'static str {
        match self {
            Path::Dest => "dest",
            Path::Home => "home",
            Path::None => "none",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<Path> {
        [Path::Dest, Path::Home, Path::None]
            .into_iter()
            .find(|path| path.serialized_name() == name)
    }

    /// `hasTarget`.
    pub fn has_target(self) -> bool {
        self != Path::None
    }

    /// `noTarget`.
    pub fn no_target(self) -> bool {
        self == Path::None
    }

    /// `isHome`.
    pub fn is_home(self) -> bool {
        self == Path::Home
    }
}

/// `SidedBlockPos`: a posição empacotada de um destino e o lado dele por onde a pilha entra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SidedBlockPos {
    pub pos: i64,
    pub side: Face,
}

/// `Direction.CODEC`: o nome do lado.
fn read_side(name: &str) -> Option<Face> {
    Face::from_property(name)
}

fn color_name(color: u8) -> &'static str {
    ENUM_COLOR_NAMES[usize::from(color).min(ENUM_COLOR_NAMES.len() - 1)]
}

fn color_from_name(name: &str) -> Option<u8> {
    ENUM_COLOR_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as u8)
}

#[derive(Clone, Debug)]
pub struct TransporterStack {
    item: Option<(ItemResource, u32)>,
    pub progress: i32,
    pub color: Option<u8>,
    pub initiated_path: bool,
    pub idle_dir: Option<Face>,
    /// `originalLocation`, `Long.MAX_VALUE` sem posição.
    pub original_location: i64,
    /// `homeLocation`, `Long.MAX_VALUE` sem posição.
    pub home_location: i64,
    client_next: i64,
    client_prev: i64,
    path_type: Option<Path>,
    path_to_target: Vec<i64>,
}

impl Default for TransporterStack {
    fn default() -> Self {
        Self::new()
    }
}

impl TransporterStack {
    pub fn new() -> Self {
        Self {
            item: None,
            progress: 0,
            color: None,
            initiated_path: false,
            idle_dir: None,
            original_location: NO_POS,
            home_location: NO_POS,
            client_next: NO_POS,
            client_prev: NO_POS,
            path_type: None,
            path_to_target: Vec::new(),
        }
    }

    /// `createInsertStack(outputterCoord, color)`: a pilha de um item que entra no transportador vindo de `outputter`, que também é a casa.
    pub fn for_insert(outputter: i64, color: Option<u8>) -> Self {
        Self {
            original_location: outputter,
            home_location: outputter,
            color,
            ..Self::new()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.item.is_none()
    }

    /// `getItemType`.
    pub fn item_type(&self) -> Option<&ItemResource> {
        self.item.as_ref().map(|(item, _)| item)
    }

    /// `size`.
    pub fn size(&self) -> u32 {
        self.item.as_ref().map_or(0, |(_, amount)| *amount)
    }

    /// `setStack(itemType, amount)`: sem item ou sem quantidade a pilha fica vazia.
    pub fn set_stack(&mut self, item: Option<&ItemResource>, amount: u32) {
        self.item = match item {
            Some(item) if amount > 0 => Some((item.clone(), amount)),
            _ => None,
        };
    }

    /// `hasPath`: o caminho tem ao menos o destino e um transportador.
    pub fn has_path(&self) -> bool {
        self.path_to_target.len() >= 2
    }

    /// `getPath`: do destino (índice 0) até o transportador onde a pilha está.
    pub fn path(&self) -> &[i64] {
        &self.path_to_target
    }

    /// `getPathType`: sem tipo guardado é `NONE`.
    pub fn path_type(&self) -> Path {
        self.path_type.unwrap_or(Path::None)
    }

    /// O `pathType` cru, nulo antes do primeiro caminho.
    pub fn raw_path_type(&self) -> Option<Path> {
        self.path_type
    }

    /// `isFinal(transporter)`: o transportador é o último antes do destino (ou o único, num caminho ocioso).
    pub fn is_final(&self, transporter: i64) -> bool {
        let index = usize::from(self.path_type().has_target());
        self.path_to_target.get(index).copied() == Some(transporter)
    }

    fn index_of(&self, pos: i64) -> Option<usize> {
        self.path_to_target.iter().position(|p| *p == pos)
    }

    /// `getNext(long pos)`.
    pub fn get_next(&self, pos: i64) -> i64 {
        // `indexOf` é -1 se não está, então o índice é -2.
        match self.index_of(pos) {
            Some(index) if index >= 1 => self.path_to_target[index - 1],
            _ => NO_POS,
        }
    }

    /// `getPrev(long pos)`.
    pub fn get_prev(&self, pos: i64) -> i64 {
        // Sem a posição, `indexOf` é -1 e o índice 0.
        let index = self.index_of(pos).map_or(0, |index| index + 1);
        if index < self.path_to_target.len() {
            self.path_to_target[index]
        } else {
            self.original_location
        }
    }

    /// `getNext(transporter)` no cliente: o que o pacote de atualização trouxe.
    pub fn client_next(&self) -> i64 {
        self.client_next
    }

    /// `getPrev(transporter)` no cliente.
    pub fn client_prev(&self) -> i64 {
        self.client_prev
    }

    /// `updateForPos(pos)`.
    pub fn update_for_pos(&mut self, pos: i64) {
        self.client_next = self.get_next(pos);
        self.client_prev = self.get_prev(pos);
    }

    /// `getSide(transporter)`: o lado em que a pilha está no transportador, o de onde ela veio na primeira metade e o para onde vai na
    /// segunda. Sem lado (posições que não são vizinhas) o original devolve `DOWN`.
    pub fn get_side(&self, transporter: i64) -> Face {
        let mut side = None;
        if self.progress < 50 {
            let prev = self.get_prev(transporter);
            if prev != NO_POS {
                side = side_difference(transporter, prev);
            }
        } else {
            let next = self.get_next(transporter);
            if next != NO_POS {
                side = side_difference(next, transporter);
            }
        }
        side.unwrap_or(Face::Down)
    }

    /// `getSide(long pos, long target)`.
    pub fn get_side_between(pos: i64, target: i64) -> Face {
        let mut side = None;
        if target != NO_POS {
            side = side_difference(target, pos);
        }
        side.unwrap_or(Face::Down)
    }

    /// `getDest`: o primeiro do caminho.
    pub fn get_dest(&self) -> i64 {
        self.path_to_target[0]
    }

    /// `getSideOfDest`: o lado do destino por onde o último transportador o toca.
    pub fn side_of_dest(&self) -> Option<Face> {
        if self.has_path() {
            let last_transporter = self.path_to_target[1];
            return side_difference(last_transporter, self.get_dest());
        }
        None
    }

    /// `canInsertToTransporter(transporter, from, transporterFrom)`: a cor do transportador é a da pilha (ou nenhuma) e os dois lados se
    /// ligam.
    pub fn can_insert_to_transporter(
        &self,
        transporter: Option<&dyn Transporter>,
        from: Face,
        transporter_from: Option<&dyn Transporter>,
    ) -> bool {
        let Some(transporter) = transporter else {
            return false;
        };
        let color = transporter.color();
        (color.is_none() || color == self.color)
            && transporter.can_connect_mutual(from.opposite(), transporter_from)
    }

    // -----------------------------------------------------------------------------------------------------------------------------------
    // NBT
    // -----------------------------------------------------------------------------------------------------------------------------------

    /// `STREAM_CODEC` e `writeCommon`: as chaves que o NBT salvo e o de atualização têm em comum.
    fn write_common(&self, out: &mut Compound) {
        if let Some(color) = self.color {
            out.insert(keys::COLOR.into(), Nbt::String(color_name(color).into()));
        }
        out.insert(keys::PROGRESS.into(), Nbt::Int(self.progress));
        out.insert(
            keys::ORIGINAL_LOCATION.into(),
            Nbt::Long(self.original_location),
        );
        if let Some((item, amount)) = &self.item {
            // O `ItemResource.CODEC` grava o id; os componentes do item ficam por conta de quem liga o jogo.
            let mut resource = Compound::new();
            resource.insert(keys::ID.into(), Nbt::String(item.id.to_string()));
            let mut stack = Compound::new();
            stack.insert(keys::TYPE.into(), Nbt::Compound(resource));
            stack.insert(keys::AMOUNT.into(), Nbt::Long(i64::from(*amount)));
            out.insert(keys::ITEM.into(), Nbt::Compound(stack));
        }
    }

    /// `write(output)`: o NBT que vai para o disco. O `pathType` é o cru (nulo não é gravado) e a casa só se existe.
    pub fn write(&self) -> Compound {
        let mut out = Compound::new();
        self.write_common(&mut out);
        if let Some(path_type) = self.path_type {
            out.insert(
                keys::PATH_TYPE.into(),
                Nbt::String(path_type.serialized_name().into()),
            );
        }
        if let Some(idle_dir) = self.idle_dir {
            out.insert(keys::IDLE_DIR.into(), Nbt::String(idle_dir.name().into()));
        }
        if self.home_location != NO_POS {
            out.insert(keys::HOME_LOCATION.into(), Nbt::Long(self.home_location));
        }
        out
    }

    /// `writeToUpdateTag(transporter, output)` no servidor: o comum, o `pathType` (`NONE` se nulo) e o próximo e o anterior do
    /// transportador `transporter`, quando existem.
    pub fn write_to_update_tag(&self, transporter: i64) -> Compound {
        let mut out = Compound::new();
        self.write_common(&mut out);
        out.insert(
            keys::PATH_TYPE.into(),
            Nbt::String(self.path_type().serialized_name().into()),
        );
        let next = self.get_next(transporter);
        if next != NO_POS {
            out.insert(keys::NEXT.into(), Nbt::Long(next));
        }
        let prev = self.get_prev(transporter);
        if prev != NO_POS {
            out.insert(keys::PREVIOUS.into(), Nbt::Long(prev));
        }
        out
    }

    /// O construtor privado `TransporterStack(ValueInput)`. `resolve` dá o `ItemResource` de um id (o limite de pilha vem do registro).
    fn from_input(input: &Compound, resolve: &dyn Fn(&str) -> Option<ItemResource>) -> Self {
        let mut stack = Self::new();
        stack.color = input
            .get(keys::COLOR)
            .and_then(Nbt::as_str)
            .and_then(color_from_name);
        stack.progress = input
            .get(keys::PROGRESS)
            .and_then(Nbt::as_int)
            .unwrap_or(0);
        stack.original_location = input
            .get(keys::ORIGINAL_LOCATION)
            .and_then(Nbt::as_long)
            .unwrap_or(NO_POS);
        stack.path_type = input
            .get(keys::PATH_TYPE)
            .and_then(Nbt::as_str)
            .and_then(Path::from_serialized_name);
        // `LargeResourceStack.ITEM_HELPER.readOrEmpty`: a pilha vazia se falta a chave, o id ou a quantidade.
        if let Some(item) = input.get(keys::ITEM).and_then(Nbt::as_compound)
            && let Some(id) = item
                .get(keys::TYPE)
                .and_then(Nbt::as_compound)
                .and_then(|resource| resource.get(keys::ID))
                .and_then(Nbt::as_str)
            && let Some(amount) = item.get(keys::AMOUNT).and_then(Nbt::as_long)
            && amount > 0
            && let Some(resource) = resolve(id)
        {
            stack.item = Some((resource, amount.min(i64::from(i32::MAX)) as u32));
        }
        stack
    }

    /// `read(input)`: a pilha salva no disco.
    pub fn read(input: &Compound, resolve: &dyn Fn(&str) -> Option<ItemResource>) -> Self {
        let mut stack = Self::from_input(input, resolve);
        stack.idle_dir = input
            .get(keys::IDLE_DIR)
            .and_then(Nbt::as_str)
            .and_then(read_side);
        stack.home_location = input
            .get(keys::HOME_LOCATION)
            .and_then(Nbt::as_long)
            .unwrap_or(NO_POS);
        stack
    }

    /// `readFromUpdate(input)`: a pilha do pacote de atualização, com o próximo e o anterior que o cliente usa.
    pub fn read_from_update(
        input: &Compound,
        resolve: &dyn Fn(&str) -> Option<ItemResource>,
    ) -> Self {
        let mut stack = Self::from_input(input, resolve);
        stack.client_next = input
            .get(keys::NEXT)
            .and_then(Nbt::as_long)
            .unwrap_or(NO_POS);
        stack.client_prev = input
            .get(keys::PREVIOUS)
            .and_then(Nbt::as_long)
            .unwrap_or(NO_POS);
        stack
    }
}

/// Uma pilha com identidade: o `TransporterManager` guarda a mesma que o transportador (um `HashSet` por identidade).
#[derive(Clone, Debug)]
pub struct StackRef(Rc<RefCell<TransporterStack>>);

impl StackRef {
    pub fn new(stack: TransporterStack) -> Self {
        Self(Rc::new(RefCell::new(stack)))
    }

    pub fn borrow(&self) -> Ref<'_, TransporterStack> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, TransporterStack> {
        self.0.borrow_mut()
    }

    /// Se é a mesma pilha (a identidade do objeto).
    pub fn same(&self, other: &StackRef) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// `setPath(world, path, type, transaction)`: tira a pilha das em voo se tinha destino, troca o caminho e põe de novo se tem.
    pub fn set_path(
        &self,
        ctx: &PathContext,
        path: Vec<i64>,
        path_type: Path,
        transaction: Option<&Transaction>,
    ) {
        let remove = self
            .0
            .borrow()
            .path_type
            .is_none_or(|current| current.has_target());
        if remove {
            ctx.manager.remove(ctx.dimension, self, transaction);
        }
        {
            let mut stack = self.0.borrow_mut();
            stack.path_to_target = path;
            stack.path_type = Some(path_type);
        }
        if path_type.has_target() {
            ctx.manager.add(ctx.dimension, self, transaction);
        }
    }

    /// `recalculatePath(level, request, transporter, min, transaction)`. `start` é o transportador onde a pilha está e `network` a rede dele.
    pub fn recalculate_path(
        &self,
        ctx: &PathContext,
        network: Option<&dyn InventoryNetworkView>,
        start: BlockPos,
        request: &TransitRequest,
        min: u32,
        transaction: Option<&Transaction>,
    ) -> TransitResponse {
        let local = self.0.borrow().clone();
        let Some(new_path) =
            get_new_base_path(ctx, network, start, &local, request, min, transaction)
        else {
            return TransitResponse::EMPTY;
        };
        self.0.borrow_mut().idle_dir = None;
        self.set_path(ctx, new_path.path().to_vec(), Path::Dest, transaction);
        self.0.borrow_mut().initiated_path = true;
        new_path.response_or_empty()
    }

    /// `recalculateRRPath(level, request, outputter, transporter, min, transaction)`.
    #[allow(clippy::too_many_arguments)]
    pub fn recalculate_rr_path(
        &self,
        ctx: &PathContext,
        network: Option<&dyn InventoryNetworkView>,
        start: BlockPos,
        request: &TransitRequest,
        outputter: &mut dyn RoundRobinEjector,
        min: u32,
        transaction: Option<&Transaction>,
    ) -> TransitResponse {
        let local = self.0.borrow().clone();
        let Some(new_path) = get_new_rr_path(
            ctx,
            network,
            start,
            &local,
            request,
            outputter,
            min,
            transaction,
        ) else {
            return TransitResponse::EMPTY;
        };
        self.0.borrow_mut().idle_dir = None;
        self.set_path(ctx, new_path.path().to_vec(), Path::Dest, transaction);
        self.0.borrow_mut().initiated_path = true;
        new_path.response_or_empty()
    }

    /// `calculateIdle(level, transporter, transaction)`: um caminho de volta para casa ou um passeio ocioso. Falso se não achou nenhum.
    pub fn calculate_idle(
        &self,
        ctx: &PathContext,
        network: Option<&dyn InventoryNetworkView>,
        start: BlockPos,
        transaction: Option<&Transaction>,
    ) -> bool {
        let mut local = self.0.borrow().clone();
        let new_path = get_idle_path(ctx, network, start, &mut local, transaction);
        {
            // A busca zera a casa que não alcança e mexe no `idleDir`, mesmo quando não acha nada.
            let mut stack = self.0.borrow_mut();
            stack.home_location = local.home_location;
            stack.idle_dir = local.idle_dir;
        }
        let Some(new_path) = new_path else {
            return false;
        };
        if new_path.path_type.is_home() {
            self.0.borrow_mut().idle_dir = None;
        }
        self.set_path(ctx, new_path.path, new_path.path_type, transaction);
        let mut stack = self.0.borrow_mut();
        stack.original_location = start.as_long();
        stack.initiated_path = true;
        true
    }
}
