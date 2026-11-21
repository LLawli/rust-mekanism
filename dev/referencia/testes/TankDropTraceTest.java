package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismRegistries;
import mekanism.api.RelativeSide;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityChemicalTank;
import mekanism.common.tile.TileEntityFluidTank;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// O que o nível deixa no chão quando ele mesmo destrói um tanque (frente `tanques`, 9.ª rodada): `Level.destroyBlock(pos, true)` (o `setblock ... destroy`, a
/// água) e a explosão (`Level.explode`, `ExplosionInteraction.TNT`). Grava `trace/tank_level_drop.jsonl`, uma linha por caso:
///
/// - `case`: o nome; `block`: o id do bloco; `how`: `destroy`, `explode` (potência 6, no centro do bloco de cima) ou `tnt` (potência 4, a da TNT, no mesmo lugar);
/// - `loot`: o que `Block.getDrops(state, level, pos, tile)` dá para o tile montado, cada pilha pelo `ItemStack.CODEC`;
/// - `broken`: se o bloco saiu; `entities`: as pilhas dos `ItemEntity` que apareceram na estrutura, pelo mesmo codec, na ordem em que o nível as devolve.
///
/// A resistência dos tanques é 16 (`BlockTile.defaultProperties`: `strength(3.5F, 16)`): um raio de uma explosão de potência 4 nasce com no máximo 5,2 e perde 0,225
/// por passo e 4,89 no bloco, então a TNT encostada não o quebra; o caso `tnt` grava isso.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class TankDropTraceTest {

    private static final int TIMEOUT = 20 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos POS = new BlockPos(3, 2, 3);

    private static ChemicalResource chemicalOf(String chemical) {
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath("mekanism", chemical));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank_drop")
    @TestHolder(description = "Records the item entities the level leaves when it destroys a Fluid Tank and a Chemical Tank itself: destroyBlock and an explosion.")
    public static void tankLevelDrop(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(7, 6, 7).fill(0, 0, 0, 6, 0, 6, Blocks.OBSIDIAN.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<String> lines = new ArrayList<>();
                  for (String how : new String[]{"destroy", "explode", "tnt"}) {
                      lines.add(run(helper, "fluid_plain_" + how, MekanismBlocks.BASIC_FLUID_TANK.value(), how, 0));
                      lines.add(run(helper, "fluid_full_" + how, MekanismBlocks.BASIC_FLUID_TANK.value(), how, 1));
                      lines.add(run(helper, "chemical_plain_" + how, MekanismBlocks.BASIC_CHEMICAL_TANK.value(), how, 0));
                      lines.add(run(helper, "chemical_full_" + how, MekanismBlocks.ADVANCED_CHEMICAL_TANK.value(), how, 1));
                  }
                  try {
                      Path file = Path.of("trace", "tank_level_drop.jsonl");
                      Files.createDirectories(file.getParent());
                      Files.write(file, lines);
                  } catch (IOException e) {
                      throw new IllegalStateException(e);
                  }
              })
              .thenSucceed()
        );
    }

    private static String run(MekGameTestHelper helper, String name, Block block, String how, int fill) {
        ServerLevel level = helper.getLevel();
        RegistryOps<JsonElement> json = RegistryOps.create(JsonOps.INSTANCE, level.registryAccess());
        BlockPos pos = helper.absolutePos(POS);
        AABB box = new AABB(helper.absolutePos(new BlockPos(0, 0, 0))).minmax(new AABB(helper.absolutePos(new BlockPos(6, 5, 6)))).inflate(4);
        for (ItemEntity old : level.getEntitiesOfClass(ItemEntity.class, box)) {
            old.discard();
        }
        level.setBlockAndUpdate(pos, block.defaultBlockState());
        if (fill != 0) {
            if (level.getBlockEntity(pos) instanceof TileEntityFluidTank tile) {
                tile.fluidTank.setContents(FluidResource.of(Fluids.LAVA), 5000, null);
                tile.getInventorySlots().get(0).setContents(ItemResource.of(new ItemStack(Items.LAVA_BUCKET)), 1, null);
                tile.getInventorySlots().get(1).setContents(ItemResource.of(new ItemStack(Items.BUCKET)), 3, null);
                tile.nextMode();
            } else if (level.getBlockEntity(pos) instanceof TileEntityChemicalTank tile) {
                tile.getChemicalTank().setContents(chemicalOf("hydrogen"), 5000, null);
                tile.getInventorySlots().get(0).setContents(ItemResource.of(new ItemStack(Items.STICK)), 2, null);
                ConfigInfo info = tile.getConfig().getConfig(TransmissionType.CHEMICAL);
                info.setDataType(DataType.OUTPUT, RelativeSide.FRONT);
                tile.getConfig().sideChanged(TransmissionType.CHEMICAL, RelativeSide.FRONT);
                info.setEjecting(true);
                tile.nextMode(0);
            }
        }
        BlockState state = level.getBlockState(pos);
        JsonObject line = new JsonObject();
        line.addProperty("case", name);
        line.addProperty("block", BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString());
        line.addProperty("how", how);
        JsonArray loot = new JsonArray();
        for (ItemStack drop : Block.getDrops(state, level, pos, level.getBlockEntity(pos))) {
            loot.add(ItemStack.CODEC.encodeStart(json, drop).getOrThrow());
        }
        line.add("loot", loot);
        switch (how) {
            case "destroy" -> level.destroyBlock(pos, true);
            case "explode" -> level.explode(null, pos.getX() + 0.5, pos.getY() + 1.5, pos.getZ() + 0.5, 6, Level.ExplosionInteraction.TNT);
            default -> level.explode(null, pos.getX() + 0.5, pos.getY() + 1.5, pos.getZ() + 0.5, 4, Level.ExplosionInteraction.TNT);
        }
        line.addProperty("broken", level.getBlockState(pos).isAir());
        JsonArray entities = new JsonArray();
        for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, box)) {
            entities.add(ItemStack.CODEC.encodeStart(json, entity.getItem()).getOrThrow());
            entity.discard();
        }
        line.add("entities", entities);
        level.removeBlockEntity(pos);
        level.setBlockAndUpdate(pos, Blocks.AIR.defaultBlockState());
        return line.toString();
    }
}
