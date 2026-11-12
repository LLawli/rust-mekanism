package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.RelativeSide;
import mekanism.api.heat.HeatAPI;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.capabilities.heat.BasicHeatCapacitor;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import mekanism.common.util.WorldUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o Heat Generator entregando energia aos vizinhos (frente `cabo-ajustes`, Fase 3): `TileEntityGenerator.onUpdateServer` faz `EnergyUtils.emit(outputCaches, energyContainer, null)`
/// com um cache só, o da frente (`ENERGY_SIDES = FRONT`), e só quando `canFunction()`; `TileEntityHeatGenerator.onUpdateServer` chama o `super` antes de queimar a lava e produzir,
/// então a energia que sai é a que o buffer tinha no fim do tick anterior. Um `.jsonl` por cenário em `trace/heat_emit_<cenário>.jsonl`, com os campos de `heat_generator_*` (`HeatTrace`
/// de `MachineTraceTest`) mais `can_function`, `redstone` (o controle) e `actors` (a energia de cada máquina posta, e a da rede de cada cabo posto). O gerador fica em (2, 0, 2),
/// virado para o leste (a frente é (3, 0, 2)). As marcas são `lava N`, `energy N`, `set NOME N` (a energia de uma máquina) e `control TIPO`. Copie o arquivo para
/// `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_heat_emit")
public class HeatEmitTraceTest {

    private static final BlockPos GENERATOR = new BlockPos(2, 0, 2);
    private static final int TIMEOUT = 30 * SharedConstants.TICKS_PER_SECOND;
    private static final long BIG = 100_000L;

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(8, 2, 5).fill(GENERATOR.getX(), 0, GENERATOR.getZ(), GENERATOR.getX(), 0, GENERATOR.getZ(),
              GeneratorsBlocks.HEAT_GENERATOR.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.EAST)));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_heat_emit")
    @TestHolder(description = "Records the Heat Generator with a machine on every side that can take energy: only the front one gets it, and what the buffer held goes out before the tick's own production.")
    public static void sides(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("sides");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.chamber("front", 3, 0, 2, BIG, 0);
                  trace.chamber("back", 1, 0, 2, BIG, 0);
                  trace.chamber("north", 2, 0, 1, BIG, 0);
                  trace.chamber("south", 2, 0, 3, BIG, 0);
                  trace.chamber("up", 2, 1, 2, BIG, 0);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("lava 10000");
                  trace.setLava(10_000);
                  trace.mark("energy 200");
                  trace.setEnergy(200);
              })
              .thenExecuteAfter(100, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_heat_emit")
    @TestHolder(description = "Records the Heat Generator with a front machine that has room for only part of the energy, then room again.")
    public static void partial(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("partial");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.chamber("front", 3, 0, 2, 100, 90);
                  trace.chamber("back", 1, 0, 2, BIG, 0);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("lava 10000");
                  trace.setLava(10_000);
              })
              .thenExecuteAfter(45, () -> {
                  trace.mark("set front 0");
                  trace.setActorEnergy("front", 0);
              })
              .thenExecuteAfter(60, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_heat_emit")
    @TestHolder(description = "Records the Heat Generator giving energy to a line of cables in front of it, with a machine at the end.")
    public static void cable(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("cable");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.cable("cable", 3, 2);
                  trace.cable("cable", 4, 2);
                  trace.cable("cable", 5, 2);
                  trace.chamber("end", 6, 0, 2, BIG, 0);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("lava 10000");
                  trace.setLava(10_000);
              })
              .thenExecuteAfter(100, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_heat_emit")
    @TestHolder(description = "Records the Heat Generator with a front machine and a cable beside it (not the front) that leads to another machine: the cable gets nothing.")
    public static void cableBeside(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("cable_beside");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.chamber("front", 3, 0, 2, BIG, 0);
                  trace.cable("beside", 2, 3);
                  trace.chamber("end", 2, 0, 4, BIG, 0);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("lava 10000");
                  trace.setLava(10_000);
              })
              .thenExecuteAfter(80, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_heat_emit")
    @TestHolder(description = "Records the Heat Generator only giving energy while canFunction() holds: redstone control HIGH without power, then disabled, then HIGH again.")
    public static void control(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("control");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.chamber("front", 3, 0, 2, BIG, 0);
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("control HIGH");
                  trace.setControl(RedstoneControl.HIGH);
                  trace.mark("lava 10000");
                  trace.setLava(10_000);
                  trace.mark("energy 100");
                  trace.setEnergy(100);
              })
              .thenExecuteAfter(40, () -> {
                  trace.mark("control DISABLED");
                  trace.setControl(RedstoneControl.DISABLED);
              })
              .thenExecuteAfter(40, () -> {
                  trace.mark("control HIGH");
                  trace.setControl(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(40, trace::write)
              .thenSucceed()
        );
    }

    private static class Trace {

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private final Map<String, BlockPos> machines = new LinkedHashMap<>();
        private final List<BlockPos> cables = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;
        private double heatBefore;

        Trace(String name) {
            this.name = name;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            heatBefore = heat();
        }

        void mark(String mark) {
            marks.add(mark);
        }

        TileEntityHeatGenerator tile() {
            return (TileEntityHeatGenerator) helper.getLevel().getBlockEntity(helper.absolutePos(GENERATOR));
        }

        double heat() {
            try {
                Field field = TileEntityHeatGenerator.class.getDeclaredField("heatCapacitor");
                field.setAccessible(true);
                return ((BasicHeatCapacitor) field.get(tile())).getHeat();
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        void setLava(long amount) {
            tile().lavaTank.setContents(amount == 0 ? FluidResource.EMPTY : FluidResource.of(Fluids.LAVA), amount, null);
        }

        void setEnergy(long energy) {
            tile().energyContainer().setEnergy(energy, null);
        }

        void setControl(RedstoneControl control) {
            tile().setControlType(control);
        }

        void setActorEnergy(String actor, long energy) {
            machine(actor).getEnergyContainer().setEnergy(energy, null);
        }

        private TileEntityMekanism machine(String actor) {
            return WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(machines.get(actor)));
        }

        /// An Enrichment Chamber that accepts energy on every side (a block put by `setBlock` has every side `NONE`: the side config of the item is what makes it accept).
        void chamber(String actor, int x, int y, int z, long max, long stored) {
            mark("chamber " + actor + " " + x + " " + y + " " + z + " " + max + " " + stored);
            helper.setBlock(x, y, z, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
            TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, y, z)));
            tile.applyComponentsFromItemStack(new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()));
            ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ENERGY);
            for (RelativeSide relative : RelativeSide.VALUES) {
                info.setDataType(DataType.INPUT, relative);
            }
            ((MachineEnergyContainer<?>) tile.getEnergyContainer()).setMaxEnergy(max);
            tile.getEnergyContainer().setEnergy(stored, null);
            machines.put(actor, new BlockPos(x, y, z));
        }

        void cable(String actor, int x, int z) {
            mark("cable " + x + " " + z);
            helper.setBlock(x, 0, z, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
            cables.add(new BlockPos(x, 0, z));
        }

        private long networkEnergy(BlockPos pos) {
            TileEntityUniversalCable cable = WorldUtils.getTileEntity(TileEntityUniversalCable.class, helper.getLevel(), helper.absolutePos(pos));
            if (cable == null) {
                return -1;
            }
            EnergyNetwork network = cable.getTransmitter().getTransmitterNetwork();
            return network == null ? -1 : network.energyContainer.getAmountAsLong();
        }

        /// Whether the front neighbour has an energy capability on the side facing the generator (`BlockCapabilityCache.getCapability` not null).
        private boolean frontHandler() {
            return helper.getLevel().getCapability(Capabilities.ENERGY.block(), helper.absolutePos(GENERATOR.east()), Direction.WEST) != null;
        }

        void record() {
            if (helper == null) {
                return;
            }
            TileEntityHeatGenerator tile = tile();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"heat_before\":").append(tick == 1 ? heatBefore : 0).append(",\"heat\":").append(heat())
                  .append(",\"temperature\":").append(tile.getTemperature())
                  .append(",\"ambient\":").append(HeatAPI.getAmbientTemp(helper.getLevel(), tile.getBlockPos()))
                  .append(",\"ambient_faces\":[");
            for (Direction side : Direction.values()) {
                line.append(side.ordinal() == 0 ? "" : ",").append(tile.getAmbientTemperature(side));
            }
            line.append("]")
                  .append(",\"epsilon\":").append(HeatAPI.EPSILON)
                  .append(",\"lava_faces\":0")
                  .append(",\"energy\":").append(tile.energyContainer().getAmountAsLong())
                  .append(",\"capacity\":").append(tile.energyContainer().getCapacityAsLong())
                  .append(",\"lava\":").append(tile.lavaTank.amountAsLong())
                  .append(",\"produced\":").append(tile.getProductionRate())
                  .append(",\"active\":").append(tile.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(tile.getBlockState()))
                  .append(",\"can_function\":").append(tile.canFunction())
                  .append(",\"control\":\"").append(tile.getControlType().name()).append('"')
                  .append(",\"front_handler\":").append(frontHandler())
                  .append(",\"actors\":{");
            boolean first = true;
            for (Map.Entry<String, BlockPos> entry : machines.entrySet()) {
                line.append(first ? "" : ",").append('"').append(entry.getKey()).append("\":").append(machine(entry.getKey()).getEnergyContainer().getAmountAsLong());
                first = false;
            }
            for (int i = 0; i < cables.size(); i++) {
                line.append(first ? "" : ",").append("\"net").append(i).append("\":").append(networkEnergy(cables.get(i)));
                first = false;
            }
            line.append("}}");
            lines.add(line.toString());
        }

        void write() {
            helper = null;
            try {
                Path file = Path.of("trace", "heat_emit_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
