package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.upgrade.IUpgradeHelper;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que o item de bloco leva quando a máquina carregada é quebrada (a tabela de loot, `Block.getDrops` com o tile) e o que o tile guarda quando o item
/// volta ao mundo (`useOn`): os componentes do item solto, o tooltip e o estado do tile de volta. `item_content_<máquina>.jsonl`, uma linha por cenário:
/// `empty` (a máquina posta sem nada), `upgrades` (só upgrades instalados e nos dois slots), `full` (tudo o que ela guarda) e `round` (a quebra do tile que o `full`
/// pôs de volta, para ver se o que sai é o que entrou).
@ForEachTest(groups = "trace")
public class ItemContentTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);

    private record Spec(String name, Supplier<ItemLike> item, boolean upgradable, int input, int output, int energy, String chemical) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(), true, 0, 1, 2, null);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), true, 0, 1, 2, null);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(), true, 0, 1, 2, null);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(), true, 1, 2, 3, "bio");
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(), false, 0, -1, 1, null);

    private static void template(DynamicTest test, Spec spec) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> new Scene(helper, spec).run());
            sequence.thenSucceed();
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_content")
    @TestHolder(description = "Records what the Enrichment Chamber item carries when broken and what the tile keeps when it is placed back.")
    public static void chamber(final DynamicTest test) {
        template(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_content")
    @TestHolder(description = "Records what the Crusher item carries when broken and what the tile keeps when it is placed back.")
    public static void crusher(final DynamicTest test) {
        template(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_content")
    @TestHolder(description = "Records what the Energized Smelter item carries when broken and what the tile keeps when it is placed back.")
    public static void smelter(final DynamicTest test) {
        template(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_content")
    @TestHolder(description = "Records what the Metallurgic Infuser item carries when broken and what the tile keeps when it is placed back.")
    public static void infuser(final DynamicTest test) {
        template(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_item_content")
    @TestHolder(description = "Records what the Heat Generator item carries when broken and what the tile keeps when it is placed back.")
    public static void heat(final DynamicTest test) {
        template(test, HEAT);
    }

    private static final class Scene {
        private final MekGameTestHelper helper;
        private final Spec spec;
        private final RegistryAccess registries;
        private final RegistryOps<JsonElement> json;
        private final List<String> lines = new ArrayList<>();
        private final Player player;

        Scene(MekGameTestHelper helper, Spec spec) {
            this.helper = helper;
            this.spec = spec;
            this.registries = helper.getLevel().registryAccess();
            this.json = RegistryOps.create(JsonOps.INSTANCE, registries);
            this.player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        }

        void run() {
            ItemStack plain = new ItemStack(spec.item.get());
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            helper.useOn(FLOOR, plain.copy(), player, Direction.UP);
            line("empty", tile(), plain);
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());

            helper.useOn(FLOOR, plain.copy(), player, Direction.UP);
            fillUpgrades(tile());
            line("upgrades", tile(), plain);
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());

            helper.useOn(FLOOR, plain.copy(), player, Direction.UP);
            fillUpgrades(tile());
            fillContents(tile());
            ItemStack dropped = line("full", tile(), plain);
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());

            helper.useOn(FLOOR, dropped.copy(), player, Direction.UP);
            line("round", tile(), dropped);
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            write("item_content_" + spec.name, lines);
        }

        private TileEntityMekanism tile() {
            BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
            return (TileEntityMekanism) entity;
        }

        private Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return registries.lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        private ItemStack upgradeItem(ResourceKey<Upgrade> key, int count) {
            return IUpgradeHelper.INSTANCE.asStack(upgrade(key), count);
        }

        private void fillUpgrades(TileEntityMekanism tile) {
            var component = tile.getComponent();
            if (spec.upgradable) {
                component.addUpgrades(registries, upgrade(UpgradeIds.SPEED), 3);
                component.addUpgrades(registries, upgrade(UpgradeIds.ENERGY), 2);
                ItemStack input = upgradeItem(UpgradeIds.SPEED, 5);
                component.getUpgradeSlot().setContents(ItemResource.of(input), input.count(), null);
                ItemStack output = upgradeItem(UpgradeIds.ENERGY, 7);
                component.getUpgradeOutputSlot().setContents(ItemResource.of(output), output.count(), null);
            } else {
                component.addUpgrades(registries, upgrade(UpgradeIds.MUFFLING), 2);
                ItemStack input = upgradeItem(UpgradeIds.MUFFLING, 3);
                component.getUpgradeSlot().setContents(ItemResource.of(input), input.count(), null);
            }
        }

        private void fillContents(TileEntityMekanism tile) {
            Item input = spec.input == 0 && spec.output < 0 ? Items.COAL : Items.COBBLESTONE;
            tile.getInventorySlots().get(spec.input).setContents(ItemResource.of(input), 17, null);
            if (spec.output >= 0) {
                tile.getInventorySlots().get(spec.output).setContents(ItemResource.of(Items.IRON_INGOT), 9, null);
            }
            tile.getInventorySlots().get(spec.energy).setContents(ItemResource.of(Items.REDSTONE), 5, null);
            if (spec.chemical != null && tile instanceof TileEntityMetallurgicInfuser machine) {
                Holder<Chemical> chemical = registries.getOrThrow(ChemicalIds.BIO);
                machine.infusionTank.setContents(ChemicalResource.of(chemical), 150, null);
            }
            if (tile instanceof TileEntityHeatGenerator generator) {
                generator.lavaTank.setContents(FluidResource.of(Fluids.LAVA), 1500, null);
                try {
                    Field field = TileEntityHeatGenerator.class.getDeclaredField("heatCapacitor");
                    field.setAccessible(true);
                    ((mekanism.common.capabilities.heat.BasicHeatCapacitor) field.get(generator)).setHeat(4000.0, null);
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(e);
                }
            }
            tile.setControlType(RedstoneControl.HIGH);
            tile.setCustomName(Component.literal("Caixa"));
            BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
            container.setEnergy(Math.min(500, container.getCapacityAsLong()), null);
        }

        /// Quebra o tile pela tabela de loot (a mesma que `Block.dropResources` rola) e grava o item solto e o estado do tile antes de sair.
        private ItemStack line(String name, TileEntityMekanism tile, ItemStack source) {
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(SPOT);
            BlockState state = level.getBlockState(abs);
            JsonObject root = new JsonObject();
            root.addProperty("scenario", name);
            root.add("tile", tileState(tile));
            List<ItemStack> drops = Block.getDrops(state, level, abs, tile);
            JsonArray array = new JsonArray();
            for (ItemStack drop : drops) {
                JsonObject entry = new JsonObject();
                entry.add("stack", ItemStack.CODEC.encodeStart(json, drop).getOrThrow());
                JsonArray tooltip = new JsonArray();
                for (Component text : drop.getTooltipLines(Item.TooltipContext.of(level), player, TooltipFlag.NORMAL)) {
                    JsonObject lineJson = new JsonObject();
                    lineJson.addProperty("text", text.getString());
                    lineJson.add("component", ComponentSerialization.CODEC.encodeStart(json, text).getOrThrow());
                    tooltip.add(lineJson);
                }
                entry.add("tooltip", tooltip);
                array.add(entry);
            }
            root.add("drops", array);
            lines.add(root.toString());
            return drops.isEmpty() ? source : drops.get(0);
        }

        private JsonObject tileState(TileEntityMekanism tile) {
            JsonObject out = new JsonObject();
            CompoundTag tag = tile.saveWithFullMetadata(registries);
            out.add("nbt", NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag));
            out.addProperty("energy", tile.getEnergyContainer().getAmountAsLong());
            out.addProperty("capacity", tile.getEnergyContainer().getCapacityAsLong());
            out.addProperty("control", tile.getControlType().name());
            if (tile.getComponent() != null) {
                JsonObject upgrades = new JsonObject();
                for (ResourceKey<Upgrade> key : List.of(UpgradeIds.SPEED, UpgradeIds.ENERGY, UpgradeIds.MUFFLING)) {
                    upgrades.addProperty(key.identifier().getPath(), tile.getComponent().getUpgrades(upgrade(key)));
                }
                out.add("upgrades", upgrades);
            }
            return out;
        }
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
