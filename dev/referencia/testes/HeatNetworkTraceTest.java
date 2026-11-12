package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.function.Supplier;
import mekanism.api.heat.HeatAPI;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.capabilities.heat.BasicHeatCapacitor;
import mekanism.common.content.network.HeatNetwork;
import mekanism.common.content.network.transmitter.ThermodynamicConductor;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityThermodynamicConductor;
import mekanism.common.util.WorldUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.RegisterStructureTemplate;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava a rede de calor (frente `rede-calor`, Fase 3), um `.jsonl` por cenário em `trace/heat_<cenário>.jsonl`, para
/// `mods/machinery_basis/core/tests/heat_network_trace.rs` repetir. Cada linha é um evento com o tick do servidor relativo ao início da gravação (`t`):
///
/// - `setup`: a origem da estrutura, a largura, e o ar (`HeatAPI.getAmbientTemp`) de cada célula da estrutura (`ambient[y][x]`, os seis lados na ordem de
///   `Direction.values()` e, no fim, a própria célula);
/// - `tile`: um condutor novo (`x`, `hash` é o `identityHashCode` do `Transmitter`, que decide a ordem do `transmittersToAdd`);
/// - `mark`: a ação do cenário (`place X tier`, `break X`, `heat X valor`, `gheat valor`), sempre no `ServerTickEvent.Pre`;
/// - `created`: uma rede nova (`net` é o índice de criação, `hash` o `UUID.hashCode()`, `from` as redes que ela adotou); `update` (`requestsUpdate` de
///   um condutor), `commit`, `invalidate` e `net_update` (o `onUpdate` da rede), na ordem em que acontecem;
/// - `state`: depois de `TransmitterNetworkRegistry.onTick` (as redes já rodaram o `onUpdate`): cada condutor (`net`, `orphan`, `valid`, `heat` e `temp`
///   sem inicializar o capacitor, `ct` é o `clientTemperature`, `all` e `acc` são os bytes de conexão), cada rede (`mean`, `lost`, `transferred`, os
///   membros na ordem do `Long2ObjectOpenHashMap`), a ordem do `networks` do registro e, no cenário `generator`, o Heat Generator (`gen`).
///
/// O estado de antes do primeiro tick sai como `begin` (mesmo formato do `state`). Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/`
/// só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_heat")
public class HeatNetworkTraceTest {

    private static final String LINE = MekanismTests.MODID + ":heat_trace_line";
    private static final int TIMEOUT = 30 * SharedConstants.TICKS_PER_SECOND;
    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final int WIDTH = 8;
    private static final String[] TIERS = {"basic", "advanced", "elite", "ultimate"};

    //Note: lazy so the conductors are registered, and so the traced block entity is in place before the structure is placed
    @RegisterStructureTemplate(LINE)
    public static final Supplier<StructureTemplate> LINE_TEMPLATE = StructureTemplateBuilder.lazy(WIDTH, 2, 1, builder -> {
        Recorder.installTracedConductors();
        return builder.fill(7, 0, 0, 7, 0, 0, Blocks.AIR.defaultBlockState());
    });

    @GameTest(template = LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_heat_line")
    @TestHolder(description = "Records a line of basic conductors with a heated end.")
    public static void line(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("line", helper);
                  Recorder.schedule(1, "pre place 0 basic", () -> place(helper, 0, "basic"));
                  for (int x = 1; x < 5; x++) {
                      int index = x;
                      Recorder.schedule(1, "pre place " + x + " basic", () -> place(helper, index, "basic"));
                  }
                  Recorder.schedule(8, "pre heat 0 5000", () -> heat(helper, 0, 5000));
                  Recorder.schedule(81, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_heat_tiers")
    @TestHolder(description = "Records a line of conductors of the four tiers, heated at both ends.")
    public static void tiers(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("tiers", helper);
                  String[] order = {"basic", "advanced", "elite", "ultimate", "elite", "advanced", "basic"};
                  for (int x = 0; x < order.length; x++) {
                      int index = x;
                      Recorder.schedule(1, "pre place " + x + " " + order[x], () -> place(helper, index, order[index]));
                  }
                  Recorder.schedule(8, "pre heat 0 5000", () -> heat(helper, 0, 5000));
                  Recorder.schedule(8, "pre heat 6 2000", () -> heat(helper, 6, 2000));
                  Recorder.schedule(81, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_heat_ambient")
    @TestHolder(description = "Records lone conductors (nothing connected) hotter and colder than the air.")
    public static void ambient(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("ambient", helper);
                  Recorder.schedule(1, "pre place 0 basic", () -> place(helper, 0, "basic"));
                  Recorder.schedule(1, "pre place 3 elite", () -> place(helper, 3, "elite"));
                  Recorder.schedule(1, "pre place 6 ultimate", () -> place(helper, 6, "ultimate"));
                  Recorder.schedule(8, "pre heat 0 5000", () -> heat(helper, 0, 5000));
                  Recorder.schedule(8, "pre heat 3 5000", () -> heat(helper, 3, 5000));
                  Recorder.schedule(8, "pre heat 6 -1000000000", () -> heat(helper, 6, -1_000_000_000));
                  Recorder.schedule(61, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_heat_merge_split")
    @TestHolder(description = "Records two heated lines that a conductor joins, is broken and is put back.")
    public static void mergeSplit(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("merge_split", helper);
                  for (int x : new int[]{0, 1, 3, 4}) {
                      Recorder.schedule(1, "pre place " + x + " basic", () -> place(helper, x, "basic"));
                  }
                  Recorder.schedule(8, "pre heat 0 5000", () -> heat(helper, 0, 5000));
                  Recorder.schedule(8, "pre heat 4 3000", () -> heat(helper, 4, 3000));
                  Recorder.schedule(14, "pre place 2 advanced", () -> place(helper, 2, "advanced"));
                  Recorder.schedule(30, "pre break 2", () -> breakAt(helper, 2));
                  Recorder.schedule(46, "pre place 2 basic", () -> place(helper, 2, "basic"));
                  Recorder.schedule(81, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_heat_generator")
    @TestHolder(description = "Records a line of conductors with a Heat Generator on top of the last one, both heated.")
    public static void generator(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("generator", helper);
                  for (int x = 0; x < 4; x++) {
                      int index = x;
                      Recorder.schedule(1, "pre place " + x + " basic", () -> place(helper, index, "basic"));
                  }
                  Recorder.schedule(1, "pre generator 3", () -> helper.setBlock(3, 1, 0, GeneratorsBlocks.HEAT_GENERATOR.defaultState()));
                  Recorder.schedule(8, "pre heat 0 5000", () -> heat(helper, 0, 5000));
                  Recorder.schedule(8, "pre gheat 2000", () -> generatorHeat(helper, 3, 2000));
                  Recorder.schedule(81, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    private static void waitFinished(MekGameTestHelper helper) {
        if (!Recorder.isFinished()) {
            helper.fail("Trace not finished yet");
        }
    }

    private static BlockState conductorState(String tier) {
        return switch (tier) {
            case "basic" -> MekanismBlocks.BASIC_THERMODYNAMIC_CONDUCTOR.defaultState();
            case "advanced" -> MekanismBlocks.ADVANCED_THERMODYNAMIC_CONDUCTOR.defaultState();
            case "elite" -> MekanismBlocks.ELITE_THERMODYNAMIC_CONDUCTOR.defaultState();
            case "ultimate" -> MekanismBlocks.ULTIMATE_THERMODYNAMIC_CONDUCTOR.defaultState();
            default -> throw new IllegalArgumentException(tier);
        };
    }

    private static void place(MekGameTestHelper helper, int x, String tier) {
        helper.setBlock(x, 0, 0, conductorState(tier));
    }

    private static void breakAt(MekGameTestHelper helper, int x) {
        helper.setBlock(x, 0, 0, Blocks.AIR);
    }

    /// Like a heater: `handleHeat` of the buffer in a root transaction.
    private static void heat(MekGameTestHelper helper, int x, double amount) {
        TileEntityThermodynamicConductor tile = WorldUtils.getTileEntity(TileEntityThermodynamicConductor.class, helper.getLevel(),
              helper.absolutePos(new BlockPos(x, 0, 0)), false);
        try (Transaction transaction = Transaction.openRoot()) {
            tile.getTransmitter().buffer.handleHeat(amount, transaction);
            transaction.commit();
        }
    }

    private static void generatorHeat(MekGameTestHelper helper, int x, double amount) {
        TileEntityHeatGenerator tile = WorldUtils.getTileEntity(TileEntityHeatGenerator.class, helper.getLevel(),
              helper.absolutePos(new BlockPos(x, 1, 0)), false);
        try (Transaction transaction = Transaction.openRoot()) {
            generatorCapacitor(tile).handleHeat(amount, transaction);
            transaction.commit();
        }
    }

    private static BasicHeatCapacitor generatorCapacitor(TileEntityHeatGenerator tile) {
        return (BasicHeatCapacitor) field(TileEntityHeatGenerator.class, "heatCapacitor", tile);
    }

    private static Object field(Class<?> owner, String name, Object instance) {
        try {
            Field field = owner.getDeclaredField(name);
            field.setAccessible(true);
            return field.get(instance);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /// The stored heat without initializing the capacitor (-1 when it is not initialized yet).
    private static double storedHeat(BasicHeatCapacitor capacitor) {
        return (double) field(BasicHeatCapacitor.class, "storedHeat", capacitor);
    }

    private static String number(double value) {
        return Double.isNaN(value) || Double.isInfinite(value) ? "null" : Double.toString(value);
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
    }

    /// The conductor block entity, with a transmitter that logs `requestsUpdate` and creates the traced network.
    private static class TracedConductor extends TileEntityThermodynamicConductor {

        TracedConductor(Holder<Block> blockProvider, BlockPos pos, BlockState state) {
            super(blockProvider, pos, state);
            Recorder.tile(pos, System.identityHashCode(getTransmitter()));
        }

        @Override
        protected ThermodynamicConductor createTransmitter(Holder<Block> blockProvider) {
            return new TracedTransmitter(blockProvider, this);
        }
    }

    private static class TracedTransmitter extends ThermodynamicConductor {

        TracedTransmitter(Holder<Block> blockProvider, TracedConductor tile) {
            super(blockProvider, tile);
        }

        @Override
        public HeatNetwork createNetworkByMerging(Collection<HeatNetwork> networks) {
            return new TracedNetwork(networks, getBlockPos());
        }

        @Override
        public void requestsUpdate() {
            Recorder.log("update", getBlockPos(), "");
            super.requestsUpdate();
        }
    }

    private static class TracedNetwork extends HeatNetwork {

        /// -1 for a network that does not belong to the structure being recorded.
        private final int index;
        /// The recording that created the network: a network that outlives its test must not write into the next recording.
        @Nullable
        private final Recorder owner = Recorder.current;

        TracedNetwork(Collection<HeatNetwork> from, BlockPos creator) {
            super(from);
            index = Recorder.nextNetworkIndex(creator);
            if (index >= 0) {
                StringBuilder merged = new StringBuilder();
                for (HeatNetwork network : from) {
                    merged.append(merged.isEmpty() ? "" : ",").append(network instanceof TracedNetwork traced ? traced.index : -1);
                }
                Recorder.event("created", "\"net\":" + index + ",\"hash\":" + getUUID().hashCode() + ",\"from\":[" + merged + "]");
            }
        }

        @Override
        public void onUpdate() {
            if (index >= 0 && Recorder.current == owner) {
                Recorder.event("net_update", "\"net\":" + index);
            }
            super.onUpdate();
        }

        @Override
        public void commit() {
            if (index >= 0 && Recorder.current == owner) {
                Recorder.event("commit", "\"net\":" + index);
            }
            super.commit();
        }

        @Override
        public void invalidate(@Nullable ThermodynamicConductor triggerTransmitter) {
            if (index >= 0 && Recorder.current == owner) {
                Recorder.event("invalidate", "\"net\":" + index);
            }
            super.invalidate(triggerTransmitter);
        }

        double meanTemp() {
            return (double) field(HeatNetwork.class, "meanTemp", this);
        }

        double heatLost() {
            return (double) field(HeatNetwork.class, "heatLost", this);
        }

        double heatTransferred() {
            return (double) field(HeatNetwork.class, "heatTransferred", this);
        }
    }

    private static class Recorder {

        @Nullable
        private static volatile Recorder current;
        private static boolean installed;

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<Scheduled> scheduled = new ArrayList<>();
        private final ServerLevel level;
        private final BlockPos origin;
        private final long startTick;
        private int networks;
        private boolean finished;

        private Recorder(String name, MekGameTestHelper helper) {
            this.name = name;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.startTick = ServerLifecycleHooks.getCurrentServer().getTickCount();
        }

        /// Puts [TracedConductor] in place of the factory of the four conductor block entity types, as a no-op while nothing is recording.
        static synchronized void installTracedConductors() {
            if (installed) {
                return;
            }
            try {
                install(MekanismTileEntityTypes.BASIC_THERMODYNAMIC_CONDUCTOR.get(), MekanismBlocks.BASIC_THERMODYNAMIC_CONDUCTOR);
                install(MekanismTileEntityTypes.ADVANCED_THERMODYNAMIC_CONDUCTOR.get(), MekanismBlocks.ADVANCED_THERMODYNAMIC_CONDUCTOR);
                install(MekanismTileEntityTypes.ELITE_THERMODYNAMIC_CONDUCTOR.get(), MekanismBlocks.ELITE_THERMODYNAMIC_CONDUCTOR);
                install(MekanismTileEntityTypes.ULTIMATE_THERMODYNAMIC_CONDUCTOR.get(), MekanismBlocks.ULTIMATE_THERMODYNAMIC_CONDUCTOR);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void install(BlockEntityType<TileEntityThermodynamicConductor> type, Holder<Block> block) throws ReflectiveOperationException {
            Field factory = BlockEntityType.class.getDeclaredField("factory");
            factory.setAccessible(true);
            BlockEntityType.BlockEntitySupplier<TileEntityThermodynamicConductor> traced = (pos, state) -> new TracedConductor(block, pos, state);
            factory.set(type, traced);
        }

        /// The index of the next network of this recording, or -1 when no recording is on or the creating conductor is not in the structure.
        static synchronized int nextNetworkIndex(BlockPos creator) {
            Recorder recorder = current;
            if (recorder == null || recorder.finished || recorder.index(creator) < 0) {
                return -1;
            }
            return recorder.networks++;
        }

        static void begin(String name, MekGameTestHelper helper) {
            Recorder recorder = new Recorder(name, helper);
            StringBuilder ambient = new StringBuilder();
            for (int y = 0; y < 2; y++) {
                StringBuilder row = new StringBuilder();
                for (int x = 0; x < WIDTH; x++) {
                    StringBuilder cell = new StringBuilder();
                    BlockPos pos = recorder.origin.offset(x, y, 0);
                    for (Direction side : Direction.values()) {
                        cell.append(cell.isEmpty() ? "" : ",").append(number(HeatAPI.getAmbientTemp(recorder.level, pos.relative(side))));
                    }
                    cell.append(',').append(number(HeatAPI.getAmbientTemp(recorder.level, pos)));
                    row.append(row.isEmpty() ? "" : ",").append('[').append(cell).append(']');
                }
                ambient.append(ambient.isEmpty() ? "" : ",").append('[').append(row).append(']');
            }
            recorder.add("setup", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                  + ",\"width\":" + WIDTH + ",\"ambient\":[" + ambient + "]");
            current = recorder;
            recorder.recordState("begin");
        }

        static synchronized void finish() {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.finished = true;
                recorder.flush();
            }
        }

        static boolean isFinished() {
            Recorder recorder = current;
            return recorder != null && recorder.finished;
        }

        static void schedule(int tick, String mark, Runnable action) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.scheduled.add(new Scheduled(tick, mark, action));
            }
        }

        static void pre() {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            long tick = recorder.relativeTick();
            for (Scheduled task : new ArrayList<>(recorder.scheduled)) {
                if (task.tick == tick) {
                    recorder.scheduled.remove(task);
                    recorder.add("mark", "\"m\":\"" + task.mark + "\"");
                    task.action.run();
                }
            }
        }

        static void post() {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.recordState("state");
            }
        }

        static void event(String kind, String fields) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.add(kind, fields);
            }
        }

        static void tile(BlockPos pos, int hash) {
            log("tile", pos, "\"hash\":" + hash);
        }

        static void log(String kind, BlockPos pos, String extra) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                int x = recorder.index(pos);
                //Only the conductors of this structure
                if (x >= 0) {
                    recorder.add(kind, "\"x\":" + x + (extra.isEmpty() ? "" : "," + extra));
                }
            }
        }

        @Nullable
        private TileEntityThermodynamicConductor conductor(int x) {
            return WorldUtils.getTileEntity(TileEntityThermodynamicConductor.class, level, origin.offset(x, 0, 0), false);
        }

        /// The index of the conductor at `pos` (the x in the structure, row y = 0), or -1 when the position is not in it.
        private int index(BlockPos pos) {
            int x = pos.getX() - origin.getX();
            return x >= 0 && x < WIDTH && pos.getZ() == origin.getZ() && pos.getY() == origin.getY() ? x : -1;
        }

        private long relativeTick() {
            return ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
        }

        private void recordState(String kind) {
            Map<Integer, TracedNetwork> nets = new TreeMap<>();
            StringBuilder conductors = new StringBuilder();
            for (int x = 0; x < WIDTH; x++) {
                conductors.append(x == 0 ? "" : ",");
                TileEntityThermodynamicConductor tile = conductor(x);
                if (tile == null) {
                    conductors.append("null");
                    continue;
                }
                ThermodynamicConductor conductor = tile.getTransmitter();
                HeatNetwork network = conductor.getTransmitterNetwork();
                int netIndex = -1;
                if (network instanceof TracedNetwork traced && traced.index >= 0) {
                    netIndex = traced.index;
                    nets.put(netIndex, traced);
                }
                double stored = storedHeat(conductor.buffer);
                double capacity = conductor.buffer.getHeatCapacity();
                conductors.append("{\"net\":").append(network == null || netIndex < 0 ? "null" : Integer.toString(netIndex))
                      .append(",\"orphan\":").append(conductor.isOrphan())
                      .append(",\"valid\":").append(conductor.isValid())
                      .append(",\"heat\":").append(stored == -1 ? "null" : number(stored))
                      .append(",\"cap\":").append(number(capacity))
                      .append(",\"ct\":").append(number((double) field(ThermodynamicConductor.class, "clientTemperature", conductor)))
                      .append(",\"all\":").append(conductor.getAllCurrentConnections() & 0xFF)
                      .append(",\"acc\":").append(conductor.getAcceptorCache().currentAcceptorConnections & 0xFF).append('}');
            }
            StringBuilder registry = new StringBuilder();
            for (DynamicNetwork<?, ?, ?> network : registryNetworks()) {
                if (network instanceof TracedNetwork traced && traced.index >= 0 && inStructure(traced)) {
                    nets.put(traced.index, traced);
                    registry.append(registry.isEmpty() ? "" : ",").append(traced.index);
                }
            }
            StringBuilder networkList = new StringBuilder();
            for (Map.Entry<Integer, TracedNetwork> entry : nets.entrySet()) {
                TracedNetwork network = entry.getValue();
                StringBuilder members = new StringBuilder();
                for (ThermodynamicConductor member : network.getTransmitters()) {
                    members.append(members.isEmpty() ? "" : ",").append(index(member.getBlockPos()));
                }
                networkList.append(networkList.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"hash\":").append(network.getUUID().hashCode())
                      .append(",\"size\":").append(network.transmittersSize())
                      .append(",\"acceptors\":").append(network.getAcceptorCount())
                      .append(",\"mean\":").append(number(network.meanTemp()))
                      .append(",\"lost\":").append(number(network.heatLost()))
                      .append(",\"transferred\":").append(number(network.heatTransferred()))
                      .append(",\"transmitters\":[").append(members).append("]}");
            }
            StringBuilder extra = new StringBuilder();
            if (name.equals("generator")) {
                BlockPos genPos = origin.offset(3, 1, 0);
                TileEntityHeatGenerator generator = WorldUtils.getTileEntity(TileEntityHeatGenerator.class, level, genPos, false);
                if (generator == null) {
                    extra.append(",\"gen\":null");
                } else {
                    double stored = storedHeat(generatorCapacitor(generator));
                    StringBuilder caps = new StringBuilder();
                    for (Direction side : Direction.values()) {
                        caps.append(caps.isEmpty() ? "" : ",").append(level.getCapability(Capabilities.HEAT, genPos, side) != null);
                    }
                    extra.append(",\"gen\":{\"heat\":").append(stored == -1 ? "null" : number(stored))
                          .append(",\"transfer\":").append(number(generator.getLastTransferLoss()))
                          .append(",\"env\":").append(number(generator.getLastEnvironmentLoss()))
                          .append(",\"caps\":[").append(caps).append("]}");
                }
            }
            add(kind, "\"cond\":[" + conductors + "],\"nets\":[" + networkList + "],\"registry\":[" + registry + "]" + extra);
            flush();
        }

        private boolean inStructure(TracedNetwork network) {
            for (ThermodynamicConductor member : network.getTransmitters()) {
                if (index(member.getBlockPos()) >= 0) {
                    return true;
                }
            }
            return false;
        }

        @SuppressWarnings("unchecked")
        private static Set<DynamicNetwork<?, ?, ?>> registryNetworks() {
            try {
                Field field = TransmitterNetworkRegistry.class.getDeclaredField("networks");
                field.setAccessible(true);
                return (Set<DynamicNetwork<?, ?, ?>>) field.get(null);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private void add(String kind, String fields) {
            lines.add("{\"t\":" + relativeTick() + ",\"e\":\"" + kind + "\"" + (fields.isEmpty() ? "" : "," + fields) + "}");
        }

        private void flush() {
            try {
                Path file = Path.of("trace", "heat_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        private record Scheduled(int tick, String mark, Runnable action) {
        }
    }
}
