package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import java.util.stream.Stream;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.MekanismRegistries;
import mekanism.api.upgrade.IUpgradeHelper;
import mekanism.api.upgrade.Upgrade;
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
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.phys.AABB;
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
/// Além desses quatro, cada slot recebe o Q (`THROW`, uma unidade e a pilha inteira), o clique do meio (`CLONE`, com o jogador no criativo) e o clique
/// duplo (`PICKUP_ALL` sobre o slot vazio, com o item em todos os outros slots da máquina e no inventário), que gravam também `machine_after`
/// (todos os slots da máquina) e, no Q, `dropped` (o que virou item no chão).
/// `slot_click_<máquina>.jsonl`. A máquina é posta por um jogador, em (1, 1, 1).
@ForEachTest(groups = "trace")
public class SlotClickTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 1, 1);
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final int CARRIED = 3;
    private static final int PRESENT = 4;
    /// Os baldes com `max_stack_size` 16 no componente (`DataComponents.MAX_STACK_SIZE`): o jogador pode segurar mais de um, e o limite do slot (`BasicInventorySlot.getLimit`
    /// é o menor entre o do slot e o `getMaxStackSize` da pilha) acompanha o componente. Varridos com os mesmos quatro cliques dos outros.
    private static final List<String> STACKED = List.of("lava_bucket+16", "water_bucket+16");
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

    private static ItemStack stack(String id, int count) {
        ItemStack stack = new ItemStack(item(id), count);
        if (id.endsWith("+16")) {
            stack.set(DataComponents.MAX_STACK_SIZE, 16);
        }
        return stack;
    }

    private static Item item(String id) {
        if (id.endsWith("+16")) {
            id = id.substring(0, id.length() - 3);
        }
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
            lines.clear();
            upgradeSweep();
            write("slot_click_upgrade_" + spec.name, lines);
        }

        /// Os dois slots virtuais da janela de upgrades (`UpgradeInventorySlot`, no menu depois dos slots da máquina): cada um dos tipos de upgrade do registro e dois
        /// itens comuns na mão, com o slot vazio, com outro tipo e com o mesmo, no clique esquerdo, no direito, na tecla numérica e no arrasto. A entrada aceita o tipo
        /// que a máquina suporta (`isValidForInsertion(.., MANUAL)`); a saída recusa tudo (`internalOnly`).
        private void upgradeSweep() {
            var registry = helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES);
            List<Holder<Upgrade>> types = registry.listElements().map(element -> (Holder<Upgrade>) element).toList();
            lines.add("{\"machine\":\"" + spec.name + "\",\"types\":" + types.size() + "}");
            var component = tile().getComponent();
            List<Object[]> slots = List.of(new Object[]{"upgrade_in", menu.getUpgradeSlot().index, component.getUpgradeSlot()},
                  new Object[]{"upgrade_out", menu.getUpgradeOutputSlot().index, component.getUpgradeOutputSlot()});
            for (Object[] slot : slots) {
                String kind = (String) slot[0];
                int at = (Integer) slot[1];
                IInventorySlot target = (IInventorySlot) slot[2];
                List<String> probes = new ArrayList<>(types.stream().map(type -> type.getRegisteredName().replace("mekanism:", "")).toList());
                probes.addAll(List.of("stick", "dirt"));
                for (String probeName : probes) {
                    for (String content : List.of("empty", "other", "same")) {
                        for (String action : List.of("left", "right", "number", "drag")) {
                            if (!content.equals("empty") && !action.equals("left") && !action.equals("number")) {
                                continue;
                            }
                            upgradeOne(types, kind, at, target, probeName, content, action);
                        }
                    }
                }
            }
        }

        private ItemStack upgradeStack(List<Holder<Upgrade>> types, String name, int count) {
            for (Holder<Upgrade> type : types) {
                if (type.getRegisteredName().equals("mekanism:" + name)) {
                    return IUpgradeHelper.INSTANCE.asResource(type).toStack(count);
                }
            }
            return new ItemStack(item(name), count);
        }

        /// O tipo do upgrade sem o namespace, ou o id do item comum.
        private static String upgradeName(ItemStack stack) {
            if (stack.isEmpty()) {
                return "";
            }
            Holder<Upgrade> type = stack.get(IUpgradeHelper.INSTANCE.dataComponent());
            return type == null ? BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath() : "upgrade:" + type.getRegisteredName().replace("mekanism:", "");
        }

        private static String upgradeJson(ItemStack stack) {
            return "[\"" + upgradeName(stack) + "\"," + stack.getCount() + "]";
        }

        private void upgradeOne(List<Holder<Upgrade>> types, String kind, int at, IInventorySlot target, String probeName, String content, String action) {
            clearAll();
            target.setContents(ItemResource.EMPTY, 0, null);
            switch (content) {
                case "other" -> {
                    ItemStack other = upgradeStack(types, probeName.equals("speed") ? "energy" : "speed", PRESENT);
                    target.setContents(ItemResource.of(other), PRESENT, null);
                }
                case "same" -> target.setContents(ItemResource.of(upgradeStack(types, probeName, 1)), PRESENT, null);
                default -> {
                }
            }
            ItemStack probe = upgradeStack(types, probeName, CARRIED);
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
                    menu.setCarried(upgradeStack(types, probeName, CARRIED * 2));
                    menu.clicked(-999, quickCraft(0, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(at, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(drainSlot, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(-999, quickCraft(2, 0), ContainerInput.QUICK_CRAFT, player);
                }
                default -> throw new IllegalStateException(action);
            }
            lines.add("{\"kind\":\"" + kind + "\",\"item\":\"" + upgradeName(probe) + "\",\"content\":\"" + content + "\",\"action\":\"" + action + "\",\"may_place\":" + mayPlace
                  + ",\"slot_after\":" + upgradeJson(menu.getSlot(at).getItem()) + ",\"carried\":" + upgradeJson(menu.getCarried()) + ",\"hotbar0\":" + upgradeJson(player.getInventory().getItem(0))
                  + ",\"drain\":" + upgradeJson(menu.getSlot(drainSlot).getItem()) + "}");
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
                for (String id : Stream.concat(ITEMS.stream(), STACKED.stream()).toList()) {
                    for (String content : List.of("empty", "other", "same")) {
                        for (String action : List.of("left", "right", "number", "drag")) {
                            if (!content.equals("empty") && !action.equals("left") && !action.equals("number")) {
                                continue;
                            }
                            one(tank, input, slot, id, content, action);
                        }
                    }
                    if (STACKED.contains(id)) {
                        continue;
                    }
                    for (String content : List.of("other", "same")) {
                        for (String action : List.of("throw", "throw_all", "clone")) {
                            one(tank, input, slot, id, content, action);
                        }
                    }
                    one(tank, input, slot, id, "empty", "double");
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
                case "same" -> target.setContents(ItemResource.of(stack(id, 1)), PRESENT, null);
                default -> {
                }
            }
            ItemStack probe = stack(id, CARRIED);
            int at = menuIndex.get(slot);
            boolean mayPlace = menu.getSlot(at).mayPlace(probe);
            boolean extended = action.equals("throw") || action.equals("throw_all") || action.equals("clone") || action.equals("double");
            int dropped = 0;
            if (extended) {
                dropped = extended(action, id, slot, at, input);
            }
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
                    menu.setCarried(stack(id, CARRIED * 2));
                    menu.clicked(-999, quickCraft(0, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(at, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(drainSlot, quickCraft(1, 0), ContainerInput.QUICK_CRAFT, player);
                    menu.clicked(-999, quickCraft(2, 0), ContainerInput.QUICK_CRAFT, player);
                }
                case "throw", "throw_all", "clone", "double" -> {
                }
                default -> throw new IllegalStateException(action);
            }
            lines.add("{\"tank\":\"" + tank + "\",\"input\":\"" + input + "\",\"slot\":" + slot + ",\"kind\":\"" + spec.kinds.get(slot) + "\",\"item\":\"" + id.replace("mekanism:", "")
                  + "\",\"content\":\"" + content + "\",\"action\":\"" + action + "\",\"may_place\":" + mayPlace + ",\"slot_after\":" + json(menu.getSlot(at).getItem())
                  + ",\"carried\":" + json(menu.getCarried()) + ",\"hotbar0\":" + json(player.getInventory().getItem(0)) + ",\"drain\":"
                  + json(menu.getSlot(drainSlot).getItem()) + (extended ? ",\"dropped\":" + dropped + ",\"machine_after\":" + machineAfter() : "") + "}");
        }

        private String machineAfter() {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < spec.kinds.size(); i++) {
                out.append(i == 0 ? "" : ",").append(json(menu.getSlot(menuIndex.get(i)).getItem()));
            }
            return out.append(']').toString();
        }

        /// O Q (`THROW`), o clique do meio (`CLONE`) e o clique duplo (`PICKUP_ALL`). Devolve quantos itens o Q largou no chão.
        private int extended(String action, String id, int slot, int at, String input) {
            IInventorySlot target = tile().getInventorySlots().get(slot);
            switch (action) {
                case "throw", "throw_all" -> {
                    //O slot guarda o item (de `one`): sem item o Q não faz nada, e o rastro só varre os conteúdos "other" e "same".
                    var level = helper.getLevel();
                    AABB all = player.getBoundingBox().inflate(3);
                    level.getEntitiesOfClass(ItemEntity.class, all).forEach(ItemEntity::discard);
                    menu.clicked(at, action.equals("throw") ? 0 : 1, ContainerInput.THROW, player);
                    int dropped = 0;
                    for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, all)) {
                        dropped += entity.getItem().getCount();
                        entity.discard();
                    }
                    return dropped;
                }
                case "clone" -> {
                    boolean before = player.getAbilities().instabuild;
                    player.getAbilities().instabuild = true;
                    try {
                        menu.clicked(at, 2, ContainerInput.CLONE, player);
                    } finally {
                        player.getAbilities().instabuild = before;
                    }
                    return 0;
                }
                case "double" -> {
                    //O slot clicado está vazio (`one` o deixou assim); os outros slots da máquina e o inventário levam o item, e o cursor traz um.
                    for (int i = 0; i < spec.kinds.size(); i++) {
                        if (i != slot) {
                            tile().getInventorySlots().get(i).setContents(ItemResource.of(item(id)), PRESENT, null);
                        }
                    }
                    player.getInventory().setItem(0, new ItemStack(item(id), 2));
                    menu.getSlot(drainSlot).set(new ItemStack(item(id), 5));
                    menu.setCarried(new ItemStack(item(id), 1));
                    menu.clicked(at, 0, ContainerInput.PICKUP_ALL, player);
                    return 0;
                }
                default -> throw new IllegalStateException(action);
            }
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
