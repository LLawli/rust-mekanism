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
import io.netty.buffer.Unpooled;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import net.minecraft.nbt.NbtIo;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import it.unimi.dsi.fastutil.objects.Object2IntMap;
import it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap;
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

/// Grava a ordem do mapa de upgrades do tile (`TileComponentUpgrade.upgrades`) para a ordem de instalação de cada subconjunto (decisão 233): todas as ordens de 2 a 4 dos oito tipos
/// (2.072 amostras), com o `hashCode` de cada `Holder<Upgrade>` da execução, a ordem do mapa do tile depois das instalações, a das chaves do `upgrades` do NBT gravado em
/// bytes e lido de volta (`NbtIo`, como o chunk), a do mapa depois de `deserialize` (o tile recarregado) e a do mapa que o cliente decodifica da janela de upgrades
/// (`SyncableStreamCodec.upgradeMap`: `Object2IntOpenHashMap::new` com o tamanho). `upgrade_history.jsonl`, uma linha por amostra.
@ForEachTest(groups = "trace")
public class UpgradeHistoryTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final List<ResourceKey<Upgrade>> ALL = List.of(UpgradeIds.SPEED, UpgradeIds.ENERGY, UpgradeIds.MUFFLING, UpgradeIds.FILTER, UpgradeIds.CHEMICAL,
          UpgradeIds.ANCHOR, UpgradeIds.STONE_GENERATOR, UpgradeIds.XP_CONTAINMENT);

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_upgrade_history")
    @TestHolder(description = "Records the order of the upgrade map of a tile for every install order, in the session, after a reload and in the synced window map.")
    public static void history(final DynamicTest test) {
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

    private static void permute(List<ResourceKey<Upgrade>> chosen, int size, List<List<ResourceKey<Upgrade>>> out) {
        if (chosen.size() == size) {
            out.add(new ArrayList<>(chosen));
            return;
        }
        for (ResourceKey<Upgrade> key : ALL) {
            if (!chosen.contains(key)) {
                chosen.add(key);
                permute(chosen, size, out);
                chosen.removeLast();
            }
        }
    }

    @SuppressWarnings("unchecked")
    private static void run(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        RegistryAccess registries = level.registryAccess();
        Player player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        ItemStack plain = new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem());
        var lookup = registries.lookupOrThrow(MekanismRegistries.Keys.UPGRADES);
        List<List<ResourceKey<Upgrade>>> samples = new ArrayList<>();
        for (int size = 2; size <= 4; size++) {
            permute(new ArrayList<>(), size, samples);
        }
        helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
        helper.useOn(FLOOR, plain.copy(), player, Direction.UP);
        TileEntityMekanism tile = (TileEntityMekanism) level.getBlockEntity(helper.absolutePos(SPOT));
        var component = tile.getComponent();
        List<String> lines = new ArrayList<>();
        try {
            Field field = mekanism.common.tile.component.TileComponentUpgrade.class.getDeclaredField("upgrades");
            field.setAccessible(true);
            net.minecraft.network.codec.StreamCodec<RegistryFriendlyByteBuf, Object2IntMap<Holder<Upgrade>>> codec = ByteBufCodecs.map(Object2IntOpenHashMap::new, Upgrade.STREAM_CODEC, ByteBufCodecs.VAR_INT);
            for (List<ResourceKey<Upgrade>> order : samples) {
                // O tile no estado em que a máquina posta nasce de um item sem upgrades: `new Object2IntOpenHashMap<>(UpgradeAware.EMPTY.upgrades())`.
                field.set(component, new Object2IntOpenHashMap<Holder<Upgrade>>(it.unimi.dsi.fastutil.objects.Object2IntMaps.<Holder<Upgrade>>emptyMap()));
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
                var tileMap = (Object2IntMap<Holder<Upgrade>>) field.get(component);
                root.add("tile", toArray(names(tileMap.keySet())));
                var buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), registries);
                codec.encode(buf, tileMap);
                root.add("window", toArray(names(codec.decode(buf).keySet())));
                CompoundTag saved;
                try (ProblemReporter.ScopedCollector reporter = new ProblemReporter.ScopedCollector(mekanism.common.Mekanism.logger)) {
                    TagValueOutput output = TagValueOutput.createWithContext(reporter, registries);
                    component.serialize(output);
                    saved = output.buildResult();
                }
                var bytes = new ByteArrayOutputStream();
                NbtIo.write(saved, new DataOutputStream(bytes));
                CompoundTag read = NbtIo.read(new DataInputStream(new ByteArrayInputStream(bytes.toByteArray())));
                root.add("file", toArray(new ArrayList<>(read.getCompoundOrEmpty("upgrades").keySet())));
                try (ProblemReporter.ScopedCollector reporter = new ProblemReporter.ScopedCollector(mekanism.common.Mekanism.logger)) {
                    component.deserialize(TagValueInput.create(reporter, registries, read));
                }
                var reloaded = (Object2IntMap<Holder<Upgrade>>) field.get(component);
                root.add("reloaded", toArray(names(reloaded.keySet())));
                lines.add(root.toString());
            }
        } catch (ReflectiveOperationException | IOException e) {
            throw new IllegalStateException(e);
        }
        try {
            Path file = Path.of("trace", "upgrade_history.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
    }

    private static JsonArray toArray(List<String> values) {
        JsonArray array = new JsonArray();
        values.forEach(array::add);
        return array;
    }
}
