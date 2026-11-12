package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava `blocos_oclusao.jsonl` (frente `oclusao`, decisões 22 e 30): por bloco do Mekanism e do Generators, por estado, o que o Horizon não deixa o mod dizer:
/// a forma de oclusão (`BlockState.getOcclusionShape`, em caixas), `canOcclude`, `isSolid`, `canBeReplaced(fluid)` (água e lava), `isPathfindable` (terra, água e ar)
/// e a forma de oclusão de cada face (`getFaceOcclusionShape`). E `blocos_oclusao_pular.jsonl`: todo par de blocos (estado padrão) em que `skipRendering` dá verdadeiro, e a
/// contagem de pares examinados, para a regra de corte de face entre os vidros ser conferida contra o original inteiro.
@ForEachTest(groups = "trace")
public class OclusaoTraceTest {

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 60 * SharedConstants.TICKS_PER_SECOND, batch = "trace_oclusao")
    @TestHolder(description = "Records the occlusion shape, canBeReplaced(fluid), isPathfindable and skipRendering of every Mekanism and Generators block.")
    public static void oclusao(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<Block> blocks = new ArrayList<>();
                  MekanismBlocks.BLOCKS.getPrimaryEntries().forEach(e -> blocks.add(e.get()));
                  GeneratorsBlocks.BLOCKS.getPrimaryEntries().forEach(e -> blocks.add(e.get()));
                  write("blocos_oclusao", blocks.stream().map(OclusaoTraceTest::block).toList());
                  List<String> skips = new ArrayList<>();
                  int pairs = 0;
                  for (Block a : blocks) {
                      for (Block b : blocks) {
                          pairs++;
                          if (a.defaultBlockState().skipRendering(b.defaultBlockState(), Direction.NORTH)) {
                              skips.add("{\"own\":" + q(id(a)) + ",\"other\":" + q(id(b)) + "}");
                          }
                      }
                  }
                  skips.add("{\"pairs\":" + pairs + ",\"blocks\":" + blocks.size() + "}");
                  write("blocos_oclusao_pular", skips);
              })
              .thenSucceed());
    }

    private static String id(Block block) {
        return BuiltInRegistries.BLOCK.getKey(block).toString();
    }

    private static String block(Block block) {
        StringBuilder out = new StringBuilder("{\"id\":").append(q(id(block))).append(",\"states\":[");
        Map<String, Integer> facts = new LinkedHashMap<>();
        boolean first = true;
        for (BlockState state : block.getStateDefinition().getPossibleStates()) {
            String fact = facts(state);
            facts.putIfAbsent(fact, facts.size());
            out.append(first ? "" : ",").append("{\"props\":{");
            boolean firstProperty = true;
            for (Property<?> property : state.getProperties()) {
                out.append(firstProperty ? "" : ",").append(q(property.getName())).append(':').append(q(name(state, property)));
                firstProperty = false;
            }
            out.append("},\"facts\":").append(facts.get(fact)).append('}');
            first = false;
        }
        return out.append("],\"facts\":[").append(String.join(",", facts.keySet())).append("]}").toString();
    }

    private static <T extends Comparable<T>> String name(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    private static String facts(BlockState state) {
        StringBuilder out = new StringBuilder("{");
        out.append("\"can_occlude\":").append(state.canOcclude());
        out.append(",\"solid\":").append(state.isSolid());
        out.append(",\"solid_render\":").append(state.isSolidRender());
        out.append(",\"occlusion\":").append(boxes(state.getOcclusionShape()));
        out.append(",\"faces\":[");
        for (int i = 0; i < Direction.values().length; i++) {
            out.append(i == 0 ? "" : ",").append(boxes(state.getFaceOcclusionShape(Direction.values()[i])));
        }
        out.append("],\"replace_water\":").append(state.canBeReplaced(Fluids.WATER));
        out.append(",\"replace_lava\":").append(state.canBeReplaced(Fluids.LAVA));
        out.append(",\"replaceable\":").append(state.canBeReplaced());
        out.append(",\"path_land\":").append(state.isPathfindable(PathComputationType.LAND));
        out.append(",\"path_water\":").append(state.isPathfindable(PathComputationType.WATER));
        out.append(",\"path_air\":").append(state.isPathfindable(PathComputationType.AIR));
        return out.append('}').toString();
    }

    private static String boxes(VoxelShape shape) {
        List<String> out = new ArrayList<>();
        for (AABB box : shape.toAabbs()) {
            out.add("[" + box.minX + "," + box.minY + "," + box.minZ + "," + box.maxX + "," + box.maxY + "," + box.maxZ + "]");
        }
        return "[" + String.join(",", out) + "]";
    }

    private static String q(String s) {
        return '"' + s.replace("\\", "\\\\").replace("\"", "\\\"") + '"';
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
