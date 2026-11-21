package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.inventory.container.MekanismContainer;
import mekanism.common.inventory.container.type.MekanismContainerType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityChemicalTank;
import mekanism.common.tile.TileEntityFluidTank;
import mekanism.common.tile.base.TileEntityMekanism;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// O shift-clique nos menus do Fluid Tank e do Chemical Tank (`MekanismContainer.quickMoveStack`), caso a caso: o menu é aberto pela fábrica registrada do tipo
/// (`MekanismContainerType.create`, com a barra de armadura), e cada caso monta o tanque, os dois slots do tile, o inventário e a armadura, chama `quickMoveStack`
/// (`direct`, com o devolvido) ou o clique inteiro (`shift`, `clicked(QUICK_MOVE)`, que repete enquanto o slot tiver o mesmo item) e grava os slots antes e depois.
/// Um arquivo por tanque: `tank_quick_move_{fluid,chemical}.jsonl`. A primeira linha é o desenho do menu (a classe e o índice de inventário de cada slot).
/// Um item de tanque sai como `id{recurso:quantidade}`.
@ForEachTest(groups = "trace")
public class TankQuickMoveTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final int MAIN_SOURCE = 20;
    private static final int HOTBAR_SOURCE = 8;
    private static final String[] LEATHER = {"leather_helmet", "leather_chestplate", "leather_leggings", "leather_boots"};

    private static void group(DynamicTest test, boolean chemical) {
        // O estado do bloco só existe depois do registro: a fábrica do molde o lê quando roda.
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1).fill(1, 0, 0, 1, 0, 0,
              chemical ? MekanismBlocks.BASIC_CHEMICAL_TANK.defaultState() : MekanismBlocks.BASIC_FLUID_TANK.defaultState()));
        Scene scene = new Scene(chemical);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(3, scene::cases)
              .thenExecuteAfter(1, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank_quick_move")
    @TestHolder(description = "Records quickMoveStack and the shift-click on every kind of slot of the Fluid Tank menu.")
    public static void tankQuickMoveFluid(final DynamicTest test) {
        group(test, false);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank_quick_move")
    @TestHolder(description = "Records quickMoveStack and the shift-click on every kind of slot of the Chemical Tank menu.")
    public static void tankQuickMoveChemical(final DynamicTest test) {
        group(test, true);
    }

    private static String q(String text) {
        return "\"" + text + "\"";
    }

    private static ChemicalResource chemicalOf(String chemical) {
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath("mekanism", chemical));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    private static FluidResource fluidOf(String fluid) {
        return FluidResource.of(fluid.equals("water") ? Fluids.WATER : Fluids.LAVA);
    }

    /// `id:quantidade`, `ftank:<fluido>:<quantidade>` e `ctank:<químico>:<quantidade>` (o item básico cheio como a aba criativa o faz, `getFilledVariant`; com
    /// `none`, o item vazio).
    private static ItemStack stack(String spec) {
        String[] part = spec.split(":");
        if (part[0].equals("ftank") || part[0].equals("ctank")) {
            boolean chemical = part[0].equals("ctank");
            Item item = (chemical ? MekanismBlocks.BASIC_CHEMICAL_TANK : MekanismBlocks.BASIC_FLUID_TANK).asItem();
            Holder<Item> holder = BuiltInRegistries.ITEM.wrapAsHolder(item);
            ItemStack stack;
            if (part[1].equals("none")) {
                stack = new ItemStack(item);
            } else if (chemical) {
                stack = ContainerType.CHEMICAL.getFilledVariant(holder, chemicalOf(part[1]), null);
            } else {
                stack = ContainerType.FLUID.getFilledVariant(holder, fluidOf(part[1]), null);
            }
            stack.setCount(Integer.parseInt(part[2]));
            return stack;
        }
        Item item = BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(part[0]));
        if (BuiltInRegistries.ITEM.getKey(item).getPath().equals("air")) {
            throw new IllegalStateException("no item " + spec);
        }
        return new ItemStack(item, Integer.parseInt(part[1]));
    }

    private static String json(ItemStack stack) {
        if (stack.isEmpty()) {
            return "[\"\",0]";
        }
        String id = BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath();
        var fluids = stack.get(MekanismDataComponents.ATTACHED_FLUIDS);
        if (fluids != null && fluids.hasNonEmptyContents()) {
            var held = fluids.containers().getFirst();
            id += "{" + BuiltInRegistries.FLUID.getKey(held.resource().getFluid()).getPath() + ":" + held.amount() + "}";
        }
        var chemicals = stack.get(MekanismDataComponents.ATTACHED_CHEMICALS);
        if (chemicals != null && chemicals.hasNonEmptyContents()) {
            var held = chemicals.containers().getFirst();
            id += "{" + held.resource().typeHolder().getRegisteredName().replace("mekanism:", "") + ":" + held.amount() + "}";
        }
        return "[\"" + id + "\"," + stack.getCount() + "," + stack.getMaxStackSize() + "]";
    }

    private static final class Scene {
        private final boolean chemical;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private MekanismContainer menu;

        Scene(boolean chemical) {
            this.chemical = chemical;
        }

        TileEntityMekanism tile() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(new BlockPos(1, 0, 0)));
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            player.getAbilities().invulnerable = true;
            MekanismContainerType<?, ?> menuType = (MekanismContainerType<?, ?>) (chemical ? MekanismContainerTypes.CHEMICAL_TANK.get() : MekanismContainerTypes.FLUID_TANK.get());
            AbstractContainerMenu opened = menuType.create(100, player.getInventory(), (Object) tile());
            if (!(opened instanceof MekanismContainer mekanism)) {
                throw new IllegalStateException("the registered factory made " + opened);
            }
            menu = mekanism;
            StringBuilder layout = new StringBuilder("{\"group\":\"layout\",\"slots\":[");
            for (Slot slot : menu.slots) {
                layout.append(slot.index == 0 ? "" : ",").append('[').append(slot.index).append(',').append(q(slot.getClass().getSimpleName())).append(',')
                      .append(slot.container == player.getInventory() ? slot.getSlotIndex() : -1).append(']');
            }
            lines.add(layout.append("]}").toString());
        }

        void write() {
            try {
                Path path = Path.of("trace", "tank_quick_move_" + (chemical ? "chemical" : "fluid") + ".jsonl");
                Files.createDirectories(path.getParent());
                Files.write(path, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        private String slotsJson() {
            StringBuilder out = new StringBuilder("[");
            for (Slot slot : menu.slots) {
                if (slot.hasItem()) {
                    out.append(out.length() == 1 ? "" : ",").append('[').append(slot.index).append(',').append(json(slot.getItem()).substring(1));
                }
            }
            return out.append(']').toString();
        }

        private int menuIndexOfInventory(int inventoryIndex) {
            for (Slot slot : menu.slots) {
                if (slot.container == player.getInventory() && slot.getSlotIndex() == inventoryIndex) {
                    return slot.index;
                }
            }
            throw new IllegalStateException("no menu slot for inventory index " + inventoryIndex);
        }

        private void setTile(int slot, String spec) {
            IInventorySlot target = tile().getInventorySlots().get(slot);
            if (spec.isEmpty()) {
                target.setContents(ItemResource.EMPTY, 0, null);
            } else {
                ItemStack stack = stack(spec);
                target.setContents(ItemResource.of(stack), stack.getCount(), null);
            }
        }

        /// `tank`: `none`, ou `<recurso>:<quantidade>`.
        private void setTank(String tank) {
            String[] part = tank.split(":");
            if (tile() instanceof TileEntityFluidTank fluidTank) {
                if (part[0].equals("none")) {
                    fluidTank.fluidTank.setContents(LargeResourceStack.FLUID_HELPER.empty(), null);
                } else {
                    fluidTank.fluidTank.setContents(fluidOf(part[0]), Long.parseLong(part[1]), null);
                }
            } else if (tile() instanceof TileEntityChemicalTank chemicalTank) {
                if (part[0].equals("none")) {
                    chemicalTank.getChemicalTank().setContents(LargeResourceStack.CHEMICAL_HELPER.empty(), null);
                } else {
                    chemicalTank.getChemicalTank().setContents(chemicalOf(part[0]), Long.parseLong(part[1]), null);
                }
            }
        }

        /// `inventory`: `empty`, `full` (pedra em todo slot), `hotbar_full` ou `partial:<item>` (dez do item no slot 2 da barra e três no slot 12 do inventário).
        private void fill(String inventory, String armor) {
            Inventory inv = player.getInventory();
            for (int i = 0; i < inv.getContainerSize(); i++) {
                inv.setItem(i, ItemStack.EMPTY);
            }
            menu.setCarried(ItemStack.EMPTY);
            player.containerMenu = menu;
            if (inventory.equals("full") || inventory.equals("hotbar_full")) {
                for (int i = 0; i < (inventory.equals("full") ? 36 : 9); i++) {
                    inv.setItem(i, stack("cobblestone:64"));
                }
            } else if (inventory.startsWith("partial:")) {
                String item = inventory.substring("partial:".length());
                inv.setItem(2, stack(item + ":10"));
                inv.setItem(12, stack(item + ":3"));
            }
            if (armor.equals("occupied")) {
                for (int k = 0; k < 4; k++) {
                    inv.setItem(39 - k, stack(LEATHER[k] + ":1"));
                }
                inv.setItem(Inventory.SLOT_OFFHAND, stack("dirt:3"));
            }
        }

        /// `source`: `main`, `hotbar`, `head`, `offhand`, `tile0` ou `tile1`.
        private void run(String mode, String source, String item, String tank, String slot0, String slot1, String inventory, String armor) {
            fill(inventory, armor);
            setTank(tank);
            setTile(0, slot0);
            setTile(1, slot1);
            int from;
            switch (source) {
                case "tile0", "tile1" -> {
                    int slot = source.equals("tile0") ? 0 : 1;
                    setTile(slot, item);
                    from = -1;
                    IInventorySlot wanted = tile().getInventorySlots().get(slot);
                    for (Slot candidate : menu.slots) {
                        if (candidate instanceof mekanism.common.inventory.container.slot.InventoryContainerSlot container && container.getInventorySlot() == wanted) {
                            from = candidate.index;
                        }
                    }
                    if (from < 0) {
                        throw new IllegalStateException("no menu slot for " + source);
                    }
                }
                default -> {
                    int index = switch (source) {
                        case "main" -> MAIN_SOURCE;
                        case "hotbar" -> HOTBAR_SOURCE;
                        case "head" -> 39;
                        case "offhand" -> Inventory.SLOT_OFFHAND;
                        default -> throw new IllegalStateException(source);
                    };
                    player.getInventory().setItem(index, stack(item));
                    from = menuIndexOfInventory(index);
                }
            }
            String before = slotsJson();
            String returned = "";
            int origin = from;
            if (mode.equals("direct")) {
                returned = ",\"returned\":" + json(menu.quickMoveStack(player, origin));
            } else {
                menu.clicked(origin, 0, ContainerInput.QUICK_MOVE, player);
            }
            lines.add("{\"group\":\"quick_move\",\"mode\":" + q(mode) + ",\"source\":" + q(source) + ",\"item\":" + q(item) + ",\"tank\":" + q(tank) + ",\"slot0\":" + q(slot0)
                  + ",\"slot1\":" + q(slot1) + ",\"inventory\":" + q(inventory) + ",\"armor\":" + q(armor) + ",\"from\":" + origin + ",\"before\":" + before + returned
                  + ",\"after\":" + slotsJson() + "}");
        }

        void cases() {
            String own = chemical ? "ctank" : "ftank";
            String a = chemical ? "hydrogen" : "water";
            String b = chemical ? "oxygen" : "lava";
            List<String> items = new ArrayList<>(List.of("water_bucket:1", "bucket:16", "bucket:1", "lava_bucket:1", "stone:10", "iron_helmet:1",
                  own + ":" + a + ":1", own + ":" + b + ":1", own + ":none:1", own + ":none:5"));
            List<String> tanks = List.of("none", a + ":5000", b + ":5000", a + ":" + (chemical ? 64_000 : 32_000));
            // O item que já está num slot do tile: o shift-clique empilha nele antes de usar um slot vazio.
            List<String[]> held = List.of(new String[]{"", ""}, new String[]{"bucket:3", ""}, new String[]{"", "bucket:3"}, new String[]{own + ":none:2", ""},
                  new String[]{"", own + ":none:2"});
            for (String mode : List.of("direct", "shift")) {
                // A. Do inventário e da barra de atalhos: o tanque e os slots do tile variam, com o inventário vazio.
                for (String source : List.of("main", "hotbar")) {
                    for (String item : items) {
                        for (String tank : tanks) {
                            for (String[] slots : held) {
                                run(mode, source, item, tank, slots[0], slots[1], "empty", "empty");
                            }
                        }
                        // E o inventário e a armadura variam, com o tanque vazio.
                        for (String inventory : List.of("full", "hotbar_full", "partial:" + item.substring(0, item.lastIndexOf(':')))) {
                            for (String armor : List.of("empty", "occupied")) {
                                run(mode, source, item, "none", "", "", inventory, armor);
                            }
                        }
                    }
                }
                // B. Dos dois slots do tile para o jogador: a armadura, a barra de atalhos e o inventário.
                for (String source : List.of("tile0", "tile1")) {
                    for (String item : List.of("bucket:5", "water_bucket:1", "iron_helmet:1", own + ":" + a + ":1", own + ":none:3", "stone:40")) {
                        String plain = item.substring(0, item.lastIndexOf(':'));
                        for (String inventory : List.of("empty", "full", "hotbar_full", "partial:" + plain)) {
                            for (String armor : List.of("empty", "occupied")) {
                                run(mode, source, item, "none", "", "", inventory, armor);
                            }
                        }
                    }
                }
                // C. Da armadura e da mão: o tile vem primeiro, depois a barra de atalhos e o inventário.
                for (String inventory : List.of("empty", "full", "hotbar_full")) {
                    run(mode, "head", "leather_helmet:1", "none", "", "", inventory, "empty");
                    for (String item : List.of("water_bucket:1", "bucket:16", "stone:10", "iron_helmet:1", own + ":" + a + ":1", own + ":none:1")) {
                        for (String tank : List.of("none", a + ":5000")) {
                            run(mode, "offhand", item, tank, "", "", inventory, "empty");
                        }
                    }
                }
            }
            fill("empty", "empty");
            setTank("none");
            setTile(0, "");
            setTile(1, "");
        }
    }
}
