package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.function.Supplier;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.config.MekanismConfig;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismCreativeTabs;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.util.EnergyUtils;
import mekanism.common.util.ItemAccessUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsCreativeTabs;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `energia-item`: grava a energia guardada no item (`ContainerType.ENERGY`, o componente `mekanism:energy`). Um `runGameTestServer`, os arquivos `item_energy_*.jsonl`:
/// `items` (a capacidade, a barra, a cor da barra e o tooltip de cada item com energia, vazio, com um pouco, pela metade e cheio, e empilhado), `creative` (o que as abas
/// criativas do Mekanism e do Generators dão para esses itens, na ordem), `machine` (a energia de uma máquina posta e quebrada: o que o item que cai leva, e o que o tile
/// guarda depois de posto um item com energia).
@ForEachTest(groups = "trace")
public class ItemEnergyTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-11";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);

    private static final List<String> ITEMS = List.of(
          "mekanism:energy_tablet", "mekanism:atomic_disassembler", "mekanism:electric_bow", "mekanism:portable_teleporter", "mekanism:free_runners",
          "mekanism:free_runners_armored", "mekanism:meka_tool", "mekanism:mekasuit_helmet", "mekanism:mekasuit_bodyarmor", "mekanism:mekasuit_pants",
          "mekanism:mekasuit_boots", "mekanism:robit", "mekanism:configurator", "mekanism:alloy_infused",
          "mekanism:enrichment_chamber", "mekanism:crusher", "mekanism:energized_smelter", "mekanism:metallurgic_infuser", "mekanismgenerators:heat_generator",
          "mekanism:basic_energy_cube", "mekanism:advanced_energy_cube", "mekanism:elite_energy_cube", "mekanism:ultimate_energy_cube", "mekanism:creative_energy_cube",
          "mekanism:basic_universal_cable", "mekanism:basic_induction_cell");

    private record Spec(String name, Supplier<ItemLike> item) {
    }

    private static final List<Spec> MACHINES = List.of(
          new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem()),
          new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem()),
          new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem()),
          new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem()),
          new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem()));

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_energy")
    @TestHolder(description = "Records the energy capacity, the bar and the tooltip of every item that stores energy, empty, partly, half, full and stacked.")
    public static void items(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> itemsAll(helper))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_energy")
    @TestHolder(description = "Records what the Mekanism and Generators creative tabs hold for the items that store energy, in order.")
    public static void creative(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> creativeAll(helper))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_energy")
    @TestHolder(description = "Records a machine with energy broken (the dropped item) and an item with energy placed (the tile's energy).")
    public static void machine(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> machineAll(helper))
              .thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Out {

        private final String name;
        private final List<String> lines = new ArrayList<>();

        Out(String name, String description, String fields) {
            this.name = name;
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", name + ".jsonl");
            meta.addProperty("description", description);
            meta.addProperty("fields", fields);
            JsonObject line = new JsonObject();
            line.add("meta", meta);
            lines.add(line.toString());
        }

        void add(JsonObject line) {
            lines.add(line.toString());
        }

        void write() {
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(net.minecraft.resources.Identifier.parse(id));
    }

    private static String id(ItemStack stack) {
        return BuiltInRegistries.ITEM.getKey(stack.getItem()).toString();
    }

    private static JsonArray patchKeys(ItemStack stack) {
        List<String> keys = new ArrayList<>();
        DataComponentPatch.SplitResult split = stack.getComponentsPatch().split();
        for (DataComponentType<?> type : split.added().keySet()) {
            keys.add(String.valueOf(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type)));
        }
        for (DataComponentType<?> type : split.removed()) {
            keys.add("!" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type));
        }
        Collections.sort(keys);
        JsonArray array = new JsonArray();
        keys.forEach(array::add);
        return array;
    }

    private static long energy(ItemStack stack) {
        return stack.isEmpty() ? 0 : ContainerType.ENERGY.getOrZero(ItemResource.of(stack));
    }

    private static EnergyHandler handler(Item item) {
        return ContainerType.ENERGY.getCapOrUnexposed(ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item)));
    }

    /// Uma pilha com `energy` pelo caminho do original: o contêiner do item guarda o valor no componente.
    private static ItemStack withEnergy(Item item, long energy) {
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        EnergyHandler handler = ContainerType.ENERGY.getCapOrUnexposed(access);
        var container = EnergyUtils.getEnergyContainer(handler);
        if (container == null) {
            return new ItemStack(item);
        }
        container.setEnergy(energy, null);
        return ItemAccessUtils.asStack(access);
    }

    private static JsonElement component(ServerLevel level, Component text) {
        return ComponentSerialization.CODEC.encodeStart(level.registryAccess().createSerializationContext(JsonOps.INSTANCE), text).getOrThrow(IllegalStateException::new);
    }

    private static JsonObject stackJson(ServerLevel level, Player player, ItemStack stack) {
        JsonObject out = new JsonObject();
        out.addProperty("item", id(stack));
        out.addProperty("count", stack.getCount());
        out.addProperty("energy", energy(stack));
        out.add("patch", patchKeys(stack));
        out.addProperty("has_component", stack.has(MekanismDataComponents.ATTACHED_ENERGY));
        out.addProperty("bar_visible", stack.isBarVisible());
        out.addProperty("bar_width", stack.getBarWidth());
        try {
            out.addProperty("bar_color", stack.getBarColor());
        } catch (IllegalStateException error) {
            // O `energyColor` é do client.toml, que o servidor de teste não carrega: o padrão do original é 0x3CFE9A.
            out.addProperty("bar_color_error", error.getMessage());
        }
        out.addProperty("ratio", ContainerType.ENERGY.getRatio(stack));
        JsonArray tooltip = new JsonArray();
        try {
            for (Component line : stack.getTooltipLines(Item.TooltipContext.of(level), player, TooltipFlag.Default.NORMAL)) {
                tooltip.add(component(level, line));
            }
            out.add("tooltip", tooltip);
        } catch (Throwable error) {
            out.addProperty("tooltip_error", error.toString());
        }
        JsonElement encoded = net.minecraft.world.item.ItemStack.CODEC.encodeStart(level.registryAccess().createSerializationContext(JsonOps.INSTANCE), stack).getOrThrow(IllegalStateException::new);
        out.add("encoded", encoded);
        return out;
    }

    private static void itemsAll(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Player player = helper.makeMockPlayerLookingAt(P1.below(), Direction.UP);
        Out out = new Out("item_energy_items",
              "Every item that stores energy (and a few that do not), at several energy levels.",
              "item; handler (the item has an energy handler: capacity, and charge/discharge by one manual insert and extract of 2^31-1 into a full and an empty stack); "
                    + "default_has_component (the default components of the item hold mekanism:energy); one line per level: level (name), stack: the item id, count, energy (ContainerType.ENERGY.getOrZero), patch (component patch keys), "
                    + "has_component, bar_visible/bar_width/bar_color (ItemStack), ratio, tooltip (the tooltip lines, normal flag, encoded as text components) or tooltip_error, encoded (the stack's codec JSON). "
                    + "The gear config values: tabletMaxEnergy and the others, as the config holds them.");
        for (String id : ITEMS) {
            Item item = item(id);
            JsonObject head = new JsonObject();
            head.addProperty("item", id);
            head.addProperty("step", "head");
            head.addProperty("default_has_component", item.components().has(MekanismDataComponents.ATTACHED_ENERGY.get()));
            EnergyHandler handler = handler(item);
            head.addProperty("handler", handler != null);
            if (handler != null) {
                head.addProperty("capacity", handler.getCapacityAsLong());
            }
            out.add(head);
            long capacity = handler == null ? 0 : handler.getCapacityAsLong();
            long[] levels = handler == null ? new long[] {0} : new long[] {0, 1, capacity / 2, Math.max(0, capacity - 1), capacity};
            String[] names = {"empty", "one", "half", "almost", "full"};
            for (int i = 0; i < levels.length; i++) {
                ItemStack stack = handler == null ? new ItemStack(item) : withEnergy(item, levels[i]);
                JsonObject line = new JsonObject();
                line.addProperty("item", id);
                line.addProperty("step", "level");
                line.addProperty("level", names[i]);
                line.add("stack", stackJson(level, player, stack));
                out.add(line);
                if (i == levels.length - 1 && handler != null) {
                    ItemStack stacked = stack.copy();
                    stacked.setCount(Math.min(2, stacked.getMaxStackSize()));
                    JsonObject two = new JsonObject();
                    two.addProperty("item", id);
                    two.addProperty("step", "stacked_full");
                    two.add("stack", stackJson(level, player, stacked));
                    out.add(two);
                }
            }
        }
        JsonObject config = new JsonObject();
        config.addProperty("step", "gear_config");
        config.addProperty("tabletMaxEnergy", MekanismConfig.gear.tabletMaxEnergy.get());
        config.addProperty("tabletChargeRate", MekanismConfig.gear.tabletChargeRate.get());
        config.addProperty("disassemblerMaxEnergy", MekanismConfig.gear.disassemblerMaxEnergy.get());
        config.addProperty("disassemblerChargeRate", MekanismConfig.gear.disassemblerChargeRate.get());
        config.addProperty("electricBowMaxEnergy", MekanismConfig.gear.electricBowMaxEnergy.get());
        config.addProperty("electricBowChargeRate", MekanismConfig.gear.electricBowChargeRate.get());
        config.addProperty("portableTeleporterMaxEnergy", MekanismConfig.gear.portableTeleporterMaxEnergy.get());
        config.addProperty("portableTeleporterChargeRate", MekanismConfig.gear.portableTeleporterChargeRate.get());
        config.addProperty("freeRunnerMaxEnergy", MekanismConfig.gear.freeRunnerMaxEnergy.get());
        config.addProperty("freeRunnerChargeRate", MekanismConfig.gear.freeRunnerChargeRate.get());
        config.addProperty("mekaToolBaseEnergyCapacity", MekanismConfig.gear.mekaToolBaseEnergyCapacity.get());
        config.addProperty("mekaToolBaseChargeRate", MekanismConfig.gear.mekaToolBaseChargeRate.get());
        config.addProperty("mekaSuitBaseEnergyCapacity", MekanismConfig.gear.mekaSuitBaseEnergyCapacity.get());
        config.addProperty("mekaSuitBaseChargeRate", MekanismConfig.gear.mekaSuitBaseChargeRate.get());
        out.add(config);
        out.write();
    }

    private static void creativeAll(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Out out = new Out("item_energy_creative",
              "The display items of the Mekanism and Generators creative tabs, restricted to the items that store energy (the ones listed in the items file).",
              "tab; index (position in the tab's display items); item; energy; patch (component patch keys); has_component; total (the last line of each tab: how many items the tab has).");
        CreativeModeTab.ItemDisplayParameters parameters = new CreativeModeTab.ItemDisplayParameters(level.enabledFeatures(), false, level.registryAccess());
        List<String> wanted = new ArrayList<>(ITEMS);
        for (var entry : List.of(
              java.util.Map.entry("mekanism", MekanismCreativeTabs.MEKANISM.get()),
              java.util.Map.entry("mekanismgenerators", GeneratorsCreativeTabs.GENERATORS.get()))) {
            CreativeModeTab tab = entry.getValue();
            tab.buildContents(parameters);
            int index = 0;
            for (ItemStack stack : tab.getDisplayItems()) {
                if (wanted.contains(id(stack))) {
                    JsonObject line = new JsonObject();
                    line.addProperty("tab", entry.getKey());
                    line.addProperty("index", index);
                    line.addProperty("item", id(stack));
                    line.addProperty("count", stack.getCount());
                    line.addProperty("energy", energy(stack));
                    line.add("patch", patchKeys(stack));
                    line.addProperty("has_component", stack.has(MekanismDataComponents.ATTACHED_ENERGY));
                    out.add(line);
                }
                index++;
            }
            JsonObject total = new JsonObject();
            total.addProperty("tab", entry.getKey());
            total.addProperty("total", index);
            out.add(total);
        }
        out.write();
    }

    private static TileEntityMekanism tile(MekGameTestHelper helper) {
        return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(P1));
    }

    private static TileEntityMekanism place(MekGameTestHelper helper, Player player, ItemStack stack) {
        helper.setBlock(P1, Blocks.AIR.defaultBlockState());
        player.setYRot(Direction.SOUTH.toYRot());
        helper.useOn(P1.below(), stack, player, Direction.UP);
        return tile(helper);
    }

    private static JsonObject tileJson(TileEntityMekanism tile) {
        JsonObject out = new JsonObject();
        if (tile == null) {
            out.addProperty("tile", false);
            return out;
        }
        out.addProperty("tile", true);
        var container = tile.getEnergyContainer();
        out.addProperty("energy", container == null ? -1 : container.getAmountAsLong());
        out.addProperty("capacity", container == null ? -1 : container.getCapacityAsLong());
        return out;
    }

    private static void machineAll(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Player player = helper.makeMockPlayerLookingAt(P1.below(), Direction.UP);
        Out out = new Out("item_energy_machine",
              "A machine placed by a player with an item that carries energy (placed: the tile's energy and capacity), then broken with Level.destroyBlock (broken: the dropped items).",
              "machine; energy_in_item (the energy component set on the item before placing, -1 for the plain item); placed (tile, energy, capacity); then the tile's energy is set to energy_set and the block is broken: "
                    + "drops (each: item, count, energy, patch, has_component), stacks_with_plain (isSameItemSameComponents against the plain item ignoring mekanism:owner), "
                    + "collect_energy (tile.collectComponents(): the mekanism:energy value, -1 when absent).");
        for (Spec spec : MACHINES) {
            Item item = spec.item.get().asItem();
            long capacity = handler(item) == null ? 0 : handler(item).getCapacityAsLong();
            long[] inItem = {-1, 0, 1, 500, capacity, capacity * 3 + 7};
            for (long value : inItem) {
                ItemStack stack = new ItemStack(item);
                if (value >= 0) {
                    stack.set(MekanismDataComponents.ATTACHED_ENERGY, value);
                }
                TileEntityMekanism placed = place(helper, player, stack);
                JsonObject line = new JsonObject();
                line.addProperty("machine", spec.name);
                line.addProperty("energy_in_item", value);
                line.add("placed", tileJson(placed));
                out.add(line);
            }
            long[] sets = {0, 1, 123, 777};
            for (long set : sets) {
                TileEntityMekanism placed = place(helper, player, new ItemStack(item));
                var container = placed.getEnergyContainer();
                if (container instanceof MachineEnergyContainer<?> machineEnergy) {
                    machineEnergy.setMaxEnergy(Math.max(machineEnergy.getCapacityAsLong(), set));
                }
                container.setEnergy(set, null);
                JsonObject line = new JsonObject();
                line.addProperty("machine", spec.name);
                line.addProperty("energy_set", set);
                line.addProperty("before_break", container.getAmountAsLong());
                Long collected = placed.collectComponents().get(MekanismDataComponents.ATTACHED_ENERGY.get());
                line.addProperty("collect_energy", collected == null ? -1 : collected);
                BlockPos abs = helper.absolutePos(P1);
                level.destroyBlock(abs, true);
                JsonArray drops = new JsonArray();
                boolean stacks = false;
                ItemStack plain = new ItemStack(item);
                for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, new AABB(abs).inflate(3))) {
                    ItemStack drop = entity.getItem();
                    JsonObject d = new JsonObject();
                    d.addProperty("item", id(drop));
                    d.addProperty("count", drop.getCount());
                    d.addProperty("energy", energy(drop));
                    d.add("patch", patchKeys(drop));
                    d.addProperty("has_component", drop.has(MekanismDataComponents.ATTACHED_ENERGY));
                    drops.add(d);
                    ItemStack copy = drop.copy();
                    copy.remove(MekanismDataComponents.OWNER.get());
                    stacks |= ItemStack.isSameItemSameComponents(copy, plain);
                    entity.discard();
                }
                line.add("drops", drops);
                line.addProperty("stacks_with_plain", stacks);
                out.add(line);
            }
        }
        out.write();
    }
}
