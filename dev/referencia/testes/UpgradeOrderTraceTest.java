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
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
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

/// Grava a ordem em que o tooltip do item de uma máquina lista os upgrades (decisão 225): o `hashCode` de cada `Holder<Upgrade>` desta execução (o NeoForge o faz igual ao da `ResourceKey`, que vale o hash de identidade: `Holder.java:281`), a ordem do mapa
/// do tile (`TileComponentUpgrade.upgrades`, um `Object2IntOpenHashMap()` de 32 posições) depois de cada instalação, a do mapa do item (`new Object2IntOpenHashMap<>(upgrades)`,
/// a tabela do tamanho do mapa), a do mapa que o cliente monta ao decodificar (`new Object2IntOpenHashMap<>()` com as entradas na ordem do item) e as linhas de upgrade do
/// tooltip. `upgrade_order.jsonl`, uma linha por amostra; rodar duas vezes mostra se a ordem muda de uma execução para outra e se ela se mantém dentro de uma.
@ForEachTest(groups = "trace")
public class UpgradeOrderTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final List<ResourceKey<Upgrade>> ALL = List.of(UpgradeIds.SPEED, UpgradeIds.ENERGY, UpgradeIds.MUFFLING, UpgradeIds.FILTER, UpgradeIds.CHEMICAL,
          UpgradeIds.ANCHOR, UpgradeIds.STONE_GENERATOR, UpgradeIds.XP_CONTAINMENT);

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_upgrade_order")
    @TestHolder(description = "Records the order of the upgrade lines in the tooltip of the broken machine item.")
    public static void order(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> run(helper));
            sequence.thenSucceed();
        });
    }

    private static List<String> names(Iterable<Holder<Upgrade>> holders) {
        List<String> out = new ArrayList<>();
        for (Holder<Upgrade> holder : holders) {
            out.add(holder.unwrapKey().orElseThrow().identifier().getPath());
        }
        return out;
    }

    @SuppressWarnings("unchecked")
    private static void run(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        RegistryAccess registries = level.registryAccess();
        Player player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        ItemStack plain = new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem());
        var lookup = registries.lookupOrThrow(MekanismRegistries.Keys.UPGRADES);
        java.util.Random random = new java.util.Random(7);
        List<String> lines = new ArrayList<>();
        for (int sample = 0; sample < 64; sample++) {
            int size = 2 + sample % 7;
            List<ResourceKey<Upgrade>> order = new ArrayList<>(ALL);
            java.util.Collections.shuffle(order, random);
            order = order.subList(0, size);
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            helper.useOn(FLOOR, plain.copy(), player, Direction.UP);
            TileEntityMekanism tile = (TileEntityMekanism) level.getBlockEntity(helper.absolutePos(SPOT));
            var component = tile.getComponent();
            for (ResourceKey<Upgrade> key : order) {
                component.addUpgrades(registries, lookup.getOrThrow(key), 1);
            }
            JsonObject root = new JsonObject();
            JsonArray installed = new JsonArray();
            order.forEach(key -> installed.add(key.identifier().getPath()));
            root.add("installed", installed);
            JsonObject hashes = new JsonObject();
            for (ResourceKey<Upgrade> key : ALL) {
                hashes.addProperty(key.identifier().getPath(), lookup.getOrThrow(key).hashCode());
            }
            root.add("hashes", hashes);
            try {
                Field field = mekanism.common.tile.component.TileComponentUpgrade.class.getDeclaredField("upgrades");
                field.setAccessible(true);
                var tileMap = (it.unimi.dsi.fastutil.objects.Object2IntMap<Holder<Upgrade>>) field.get(component);
                root.add("tile", toArray(names(tileMap.keySet())));
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
            BlockState state = level.getBlockState(helper.absolutePos(SPOT));
            List<ItemStack> drops = Block.getDrops(state, level, helper.absolutePos(SPOT), tile);
            ItemStack drop = drops.get(0);
            var aware = drop.get(mekanism.common.registries.MekanismDataComponents.UPGRADES);
            root.add("item", toArray(names(aware.upgrades().keySet())));
            var decoded = new it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap<Holder<Upgrade>>();
            for (var entry : aware.upgrades().object2IntEntrySet()) {
                decoded.put(entry.getKey(), entry.getIntValue());
            }
            root.add("decoded", toArray(names(decoded.keySet())));
            JsonArray tooltip = new JsonArray();
            for (Component text : drop.getTooltipLines(Item.TooltipContext.of(level), player, TooltipFlag.NORMAL)) {
                String line = text.getString();
                if (line.startsWith("- ")) {
                    tooltip.add(line);
                }
            }
            root.add("tooltip", tooltip);
            lines.add(root.toString());
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
        }
        try {
            Path file = Path.of("trace", "upgrade_order.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static JsonArray toArray(List<String> values) {
        JsonArray array = new JsonArray();
        values.forEach(array::add);
        return array;
    }
}
