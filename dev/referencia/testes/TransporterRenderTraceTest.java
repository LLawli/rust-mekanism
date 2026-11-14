package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.longs.LongArrayList;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.content.network.transmitter.LogisticalTransporterBase;
import mekanism.common.content.transporter.TransporterStack;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityLogisticalTransporterBase;
import mekanism.common.util.EnumUtils;
import mekanism.common.util.TransporterUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.joml.Vector3f;

/// Rastro da frente `transportador-visual` (2026-11-14): a posição que `RenderLogisticalTransporter` dá a uma pilha em trânsito, para o teste em Rust
/// (`mods/machinery_basis/core/tests/transporter_render_reference.rs`) conferir. `transporter_render_position.jsonl`, uma lista por linha `[speed, prev, next, progress, partial, side, x, y, z]` (os `float` em bits): para cada tier (`getSpeed`), cada lado de onde a pilha veio
/// (`prev`, o `originalLocation`) e para onde vai (`next`, o primeiro do caminho), cada progresso e cada `partialTick`, o `TransporterStack.getSide` e o `TransporterUtils.getStackPosition`
/// do original, com os bits dos `float` (`partial` é `partialTick * tier.getSpeed()`, como o `extractRenderState` o calcula). `prev` e `next` valem -1 sem posição (`Long.MAX_VALUE`).
@ForEachTest(groups = "trace")
public class TransporterRenderTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final int[] PROGRESS = {0, 1, 5, 25, 49, 50, 51, 75, 99, 100};
    private static final float[] PARTIAL_TICKS = {0.0F, 0.25F, 0.5F, 0.99F, 1.0F};

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_transporter_render")
    @TestHolder(description = "Records the position RenderLogisticalTransporter gives an in-transit stack for every side pair, progress and partial tick.")
    public static void transporterRender(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(4, 1, 1));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  BlockPos[] positions = {
                        new BlockPos(0, 0, 0), new BlockPos(1, 0, 0), new BlockPos(2, 0, 0), new BlockPos(3, 0, 0)
                  };
                  List<String> lines = new ArrayList<>();
                  try {
                      Field path = TransporterStack.class.getDeclaredField("pathToTarget");
                      path.setAccessible(true);
                      for (BlockPos relative : positions) {
                          BlockState state = switch (relative.getX()) {
                              case 0 -> MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState();
                              case 1 -> MekanismBlocks.ADVANCED_LOGISTICAL_TRANSPORTER.defaultState();
                              case 2 -> MekanismBlocks.ELITE_LOGISTICAL_TRANSPORTER.defaultState();
                              default -> MekanismBlocks.ULTIMATE_LOGISTICAL_TRANSPORTER.defaultState();
                          };
                          helper.setBlock(relative, state);
                          LogisticalTransporterBase transmitter = ((TileEntityLogisticalTransporterBase) helper.getBlockEntity(relative, TileEntityLogisticalTransporterBase.class)).getTransmitter();
                          BlockPos at = helper.absolutePos(relative);
                          int speed = transmitter.tier.getSpeed();
                          for (int prev = -1; prev < 6; prev++) {
                              for (int next = -1; next < 6; next++) {
                                  for (int progress : PROGRESS) {
                                      for (float partialTick : PARTIAL_TICKS) {
                                          TransporterStack stack = new TransporterStack();
                                          stack.progress = progress;
                                          stack.originalLocation = prev < 0 ? Long.MAX_VALUE : at.relative(EnumUtils.DIRECTIONS[prev]).asLong();
                                          LongArrayList list = new LongArrayList();
                                          if (next >= 0) {
                                              list.add(at.relative(EnumUtils.DIRECTIONS[next]).asLong());
                                          }
                                          list.add(at.asLong());
                                          path.set(stack, list);
                                          float partial = partialTick * speed;
                                          Direction side = stack.getSide(transmitter);
                                          Vector3f position = TransporterUtils.getStackPosition(transmitter, stack, partial);
                                          lines.add("[" + speed + "," + prev + "," + next + "," + progress + "," + Float.floatToRawIntBits(partial) + "," + side.ordinal()
                                                + "," + Float.floatToRawIntBits(position.x()) + "," + Float.floatToRawIntBits(position.y()) + "," + Float.floatToRawIntBits(position.z()) + "]");
                                      }
                                  }
                              }
                          }
                      }
                      Path directory = Path.of("trace");
                      Files.createDirectories(directory);
                      Files.write(directory.resolve("transporter_render_position.jsonl"), lines);
                  } catch (ReflectiveOperationException | IOException e) {
                      throw new IllegalStateException(e);
                  }
              })
              .thenSucceed());
    }
}
