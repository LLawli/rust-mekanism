package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.content.network.transmitter.UniversalCable;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o estado da rede a cada tick, depois de `TransmitterNetworkRegistry.onTick`, para os testes em Rust compararem.
/// Cada linha do `.jsonl` é um tick. A rede sai como o índice de quem apareceu primeiro, porque o UUID muda a cada execução.
@ForEachTest(groups = "trace")
public class TickTraceTest {

    private static final int CABLES = 5;

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 10 * SharedConstants.TICKS_PER_SECOND, batch = "trace")
    @TestHolder(description = "Records a cable network while it is split by a broken cable and merged again.")
    public static void energyCableSplit(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(CABLES, 1, 1)
              .fill(0, 0, 0, CABLES - 1, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState()));
        Trace trace = new Trace("energy_cable_split");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("insert 1000");
                  trace.cable(0).getTransmitter().getTransmitterNetworkNN().energyContainer.setEnergy(1_000, null);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("break 2");
                  helper.setBlock(2, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("place 2");
                  helper.setBlock(2, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    private static class Trace {

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;

        Trace(String name) {
            this.name = name;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
        }

        void mark(String mark) {
            marks.add(mark);
        }

        TileEntityUniversalCable cable(int x) {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(new BlockPos(x, 0, 0)));
            return tile instanceof TileEntityUniversalCable cable ? cable : null;
        }

        void record() {
            if (helper == null) {
                return;
            }
            Map<UUID, Integer> networks = new HashMap<>();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"cables\":[");
            for (int x = 0; x < CABLES; x++) {
                line.append(x == 0 ? "" : ",");
                TileEntityUniversalCable tile = cable(x);
                if (tile == null) {
                    line.append("null");
                    continue;
                }
                UniversalCable cable = tile.getTransmitter();
                line.append("{\"share\":").append(cable.getShare());
                if (cable.hasTransmitterNetwork()) {
                    EnergyNetwork network = cable.getTransmitterNetworkNN();
                    line.append(",\"net\":").append(networks.computeIfAbsent(network.getUUID(), _ -> networks.size()))
                          .append(",\"buffer\":").append(network.getBuffer())
                          .append(",\"capacity\":").append(network.getCapacity())
                          .append(",\"scale\":").append(network.currentScale);
                }
                line.append('}');
            }
            lines.add(line.append("]}").toString());
        }

        void write() {
            helper = null;
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
