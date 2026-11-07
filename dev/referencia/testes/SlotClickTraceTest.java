package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.common.inventory.container.slot.InventoryContainerSlot;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsContainerTypes;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que o clique do cursor faz em cada slot das quatro máquinas e do Heat Generator, no menu de verdade
/// (`MekanismTileContainer.clicked`): `mayPlace` e o resultado do clique esquerdo, do direito, da tecla numérica e do arrasto, com o slot
/// vazio, com outro item e com o mesmo item, para cada item de uma lista que as regras separam. O Metallurgic Infuser repete tudo com
/// cada estado do tanque de infusão e do slot de entrada, porque as regras dos dois slots olham um pelo outro.
/// `slot_click_<máquina>.jsonl`. A máquina é posta por um jogador, em (1, 1, 1).
@ForEachTest(groups = "trace")
public class SlotClickTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 1, 1);
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final int CARRIED = 3;
    private static final int PRESENT = 4;
    private static final List<String> ITEMS = List.of("stick", "dirt", "clay", "cobblestone", "sand", "redstone", "redstone_block", "coal",
          "copper_ingot", "iron_ingot", "mekanism:bio_fuel", "lava_bucket", "water_bucket", "bucket", "diamond");

    private record Spec(String name, Supplier<ItemLike> item, Supplier<ContainerTypeRegistryObject<?>> menu, List<String> kinds, String output) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismContainerTypes.ENRICHMENT_CHAMBER, List.of("input", "output", "energy"), "clay_ball");
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), () -> MekanismContainerTypes.CRUSHER,
          List.of("input", "output", "energy"), "gravel");
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(),
          () -> MekanismContainerTypes.ENERGIZED_SMELTER, List.of("input", "output", "energy"), "glass");
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismContainerTypes.METALLURGIC_INFUSER, List.of("infusion", "input", "output", "energy"), "podzol");
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(),
          () -> GeneratorsContainerTypes.HEAT_GENERATOR, List.of("fuel", "energy"), null);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slot_click")
    @TestHolder(description = "Records cursor clicks on every slot of the Enrichment Chamber menu.")
    public static void chamberClicks(final DynamicTest test) {
        clicks(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slot_click")
    @TestHolder(description = "Records cursor clicks on every slot of the Crusher menu.")
    public static void crusherClicks(final DynamicTest test) {
        clicks(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slot_click")
    @TestHolder(description = "Records cursor clicks on every slot of the Energized Smelter menu.")
    public static void smelterClicks(final DynamicTest test) {
        clicks(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slot_click")
    @TestHolder(description = "Records cursor clicks on every slot of the Metallurgic Infuser menu, with each tank and input state.")
    public static void infuserClicks(final DynamicTest test) {
        clicks(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slot_click")
    @TestHolder(description = "Records cursor clicks on every slot of the Heat Generator menu, with an empty and a full lava tank.")
    public static void heatClicks(final DynamicTest test) {
        clicks(test, HEAT);
    }

    private static void clicks(DynamicTest test, Spec spec) {
        template(test);
        Scene scene = new Scene(spec);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.run(helper))
              .thenSucceed());
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    private static String json(ItemStack stack) {
        return "[\"" + (stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()) + "\"," + stack.getCount() + "]";
    }

    private static final class Scene {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private MekanismTileContainer<TileEntityMekanism> menu;
        /// O número no menu de cada slot da máquina, na ordem de `getInventorySlots()`.
        private final Map<Integer, Integer> menuIndex = new HashMap<>();
        private int drainSlot;

        Scene(Spec spec) {
            this.spec = spec;
        }

        TileEntityMekanism tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
            return (TileEntityMekanism) tile;
        }

        void run(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(1, 0, 1, Direction.UP);
            helper.useOn(new BlockPos(1, 0, 1), new ItemStack(spec.item.get()), player, Direction.UP);
            menu = new MekanismTileContainer<>(spec.menu.get(), 100, player.getInventory(), tile());
            player.containerMenu = menu;
            List<IInventorySlot> slots = tile().getInventorySlots();
            if (slots.size() != spec.kinds.size()) {
                throw new IllegalStateException(spec.name + " has " + slots.size() + " inventory slots, expected " + spec.kinds.size());
            }
            for (Slot slot : menu.slots) {
                if (slot instanceof InventoryContainerSlot container) {
                    int index = slots.indexOf(container.getInventorySlot());
                    if (index >= 0) {
                        menuIndex.put(index, slot.index);
                    }
                }
            }
            if (menuIndex.size() != spec.kinds.size()) {
                throw new IllegalStateException(spec.name + " menu maps " + menuIndex.size() + " slots, expected " + spec.kinds.size());
            }
            lines.add("{\"machine\":\"" + spec.name + "\",\"slots\":" + spec.kinds.size() + ",\"menu_slots\":" + menu.slots.size() + "}");
            for (int i = 0; i < menu.slots.size(); i++) {
                if (menu.slots.get(i).container == player.getInventory() && menu.slots.get(i).getContainerSlot() == 9) {
                    drainSlot = i;
                    break;
                }
            }
            if (spec == INFUSER) {
                for (String tank : List.of("empty", "bio", "fungi")) {
                    for (String input : List.of("", "dirt")) {
                        sweep(tank, input);
                    }
                }
            } else if (spec == HEAT) {
                for (String tank : List.of("empty", "lava")) {
                    sweep(tank, "");
                }
            } else {
                sweep("-", "");
            }
            write("slot_click_" + spec.name, lines);
        }

        private void setTank(String tank) {
            if (spec == INFUSER) {
                var infusion = ((TileEntityMetallurgicInfuser) tile()).infusionTank;
                if (tank.equals("empty")) {
                    infusion.setContents(ChemicalResource.EMPTY, 0, null);
                } else {
                    Holder<Chemical> chemical = helper.getLevel().registryAccess().getOrThrow(tank.equals("bio") ? ChemicalIds.BIO : ChemicalIds.FUNGI);
                    infusion.setContents(ChemicalResource.of(chemical), 500, null);
                }
            } else if (spec == HEAT) {
                var lava = ((TileEntityHeatGenerator) tile()).lavaTank;
                if (tank.equals("empty")) {
                    lava.setContents(FluidResource.EMPTY, 0, null);
                } else {
                    lava.setContents(FluidResource.of(Fluids.LAVA), lava.capacityAsLong(FluidResource.of(Fluids.LAVA)), null);
                }
            }
        }

        private void clearAll() {
            for (IInventorySlot slot : tile().getInventorySlots()) {
                slot.setContents(ItemResource.EMPTY, 0, null);
            }
            for (int i = 0; i < player.getInventory().getContainerSize(); i++) {
                player.getInventory().setItem(i, ItemStack.EMPTY);
            }
            menu.setCarried(ItemStack.EMPTY);
        }

        private void sweep(String tank, String input) {
            for (int slot = 0; slot < spec.kinds.size(); slot++) {
                for (String id : ITEMS) {
                    for (String content : List.of("empty", "other", "same")) {
                        for (String action : List.of("left", "right", "number", "drag")) {
                            if (!content.equals("empty") && !action.equals("left") && !action.equals("number")) {
                                continue;
                            }
                            one(tank, input, slot, id, content, action);
                        }
                    }
                }
            }
        }

        private void one(String tank, String input, int slot, String id, String content, String action) {
            clearAll();
            setTank(tank);
            int inputIndex = spec.kinds.indexOf("input");
            if (!input.isEmpty()) {
                tile().getInventorySlots().get(inputIndex).setContents(ItemResource.of(item(input)), 1, null);
            }
            IInventorySlot target = tile().getInventorySlots().get(slot);
            // O slot de entrada que a varredura enche não pode ser o que ela testa.
            if (slot == inputIndex && !input.isEmpty()) {
                target.setContents(ItemResource.EMPTY, 0, null);
            }
            switch (content) {
                case "other" -> target.setContents(ItemResource.of(item("stick")), PRESENT, null);
                case "same" -> target.setContents(ItemResource.of(item(id)), PRESENT, null);
                default -> {
                }
            }
            ItemStack probe = new ItemStack(item(id), CARRIED);
            int at = menuIndex.get(slot);
            boolean mayPlace = menu.getSlot(at).mayPlace(probe);
            switch (action) {
                case "left" -> {
                    menu.setCarried(probe.copy());
                    menu.clicked(at, 0, ContainerInput.PICKUP, player);
                }
                case "right" -> {
                    menu.setCarried(probe.copy());
                    menu.clicked(at, 1, ContainerInput.PICKUP, player);
                }
                case "number" -> {
                    player.getInventory().setItem(0, probe.copy());
                    menu.clicked(at, 0, ContainerInput.SWAP, player);
                }
                case "drag" -> {
                    menu.setCarried(new ItemStack(item(id), CARRIED * 2));
                    menu.clicked(-999, quickCraft(0, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(at, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(drainSlot, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(-999, quickCraft(2, 0), ContainerInput.QUICK_CRAFT, player);
                }
                default -> throw new IllegalStateException(action);
            }
            lines.add("{\"tank\":\"" + tank + "\",\"input\":\"" + input + "\",\"slot\":" + slot + ",\"kind\":\"" + spec.kinds.get(slot) + "\",\"item\":\"" + id.replace("mekanism:", "")
                  + "\",\"content\":\"" + content + "\",\"action\":\"" + action + "\",\"may_place\":" + mayPlace + ",\"slot_after\":" + json(menu.getSlot(at).getItem())
                  + ",\"carried\":" + json(menu.getCarried()) + ",\"hotbar0\":" + json(player.getInventory().getItem(0)) + ",\"drain\":"
                  + json(menu.getSlot(drainSlot).getItem()) + "}");
        }

        /// `AbstractContainerMenu.getQuickcraftMask`: a fase (0 começa, 1 soma um slot, 2 termina) e o tipo (0 reparte, 1 põe um em cada).
        private static int quickCraft(int stage, int type) {
            return stage & 3 | (type & 3) << 2;
        }

        private static void write(String name, List<String> lines) {
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
