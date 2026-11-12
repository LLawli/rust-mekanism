package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Supplier;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.level.storage.ValueOutput;
import net.neoforged.neoforge.capabilities.ICapabilityInvalidationListener;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.ChunkTicketLevelUpdatedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.RegisterStructureTemplate;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava a ordem dos eventos do jogo que a rede de transmissores recebe, um `.jsonl` por cenário em `trace/transmitter_events_<cenário>.jsonl`
/// (frente `rede-ganchos`, Fase 3). Cada linha é um evento com o tick do servidor relativo ao início do cenário (`t`) e, nos eventos de um cabo, a
/// posição relativa à estrutura (`p`: x, y, z):
///
/// - do block entity do cabo (uma subclasse de `TileEntityUniversalCable` que entra no lugar da fábrica do tipo): `clearRemoved`, `onLoad`,
///   `onAdded` (o `setPlacedBy` do bloco), `tick` (o `onUpdateServer`), `neighbor` (o `onNeighborBlockChange` que o `neighborChanged` do bloco manda,
///   com `side` `null` ou o lado), `onChunkUnloaded` e `setRemoved`;
/// - `cap`: a invalidação da capability de energia de cada vizinho do cabo (`side` é o lado do cabo em que o vizinho está), recebida por um
///   ouvinte do level (`registerCapabilityListener`) que o cabo cria em `setLevel`, no lugar do `BlockCapabilityCache` do `AcceptorCache`, que só avisa de novo depois de um `getCapability`;
/// - `ticket`, `load` e `unload`: `ChunkTicketLevelUpdatedEvent` e `ChunkEvent` dos chunks do cenário (`cx`, `cz` relativos ao chunk da origem);
/// - `pre` e `post`: `ServerTickEvent.Pre` e `.Post`; o `post` leva `runs`, o `tickRateManager().runsNormally()` que o registro lê;
/// - `mark`: o que o teste está fazendo.
///
/// O arquivo é regravado a cada tick, para sobrar mesmo se o cenário falhar.
@ForEachTest(groups = "trace_transmitter_events")
public class TransmitterEventsTraceTest {

    private static final String FLOOR = MekanismTests.MODID + ":transmitter_events_floor";
    private static final String LINE = MekanismTests.MODID + ":transmitter_events_line";
    private static final int CHUNKS = 3;
    private static final int LINE_LENGTH = CHUNKS * 16;
    private static final int TIMEOUT = 20 * SharedConstants.TICKS_PER_SECOND;

    @RegisterStructureTemplate(FLOOR)
    public static final Supplier<StructureTemplate> FLOOR_TEMPLATE = StructureTemplateBuilder.lazy(7, 3, 5, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, 6, 0, 4, Blocks.STONE.defaultBlockState());
    });

    @RegisterStructureTemplate(LINE)
    public static final Supplier<StructureTemplate> LINE_TEMPLATE = StructureTemplateBuilder.lazy(LINE_LENGTH, 3, 3, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, LINE_LENGTH - 1, 0, 2, Blocks.STONE.defaultBlockState());
    });

    private static BlockState cable() {
        return MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState();
    }

    private static BlockState machine() {
        return MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
    }

    /// Um passo do roteiro: a operação roda no `ServerTickEvent.Pre` do tick `tick` (relativo ao início), antes dos tickers do nível, que é onde a ação de um jogador
    /// (um pacote de colocar bloco, um comando) cai no servidor. O `GameTest` roda depois dos tickers e antes do `Post`, o que daria outra ordem para o registro.
    private record Step(long tick, Runnable action) {
    }

    private static final class Script {

        private static List<Step> steps = List.of();
        private static long start;
        private static long end;
        static volatile boolean done = true;

        static void start(List<Step> steps, long end) {
            Script.steps = steps;
            Script.end = end;
            start = ServerLifecycleHooks.getCurrentServer().getTickCount();
            done = false;
        }

        static void onPre(MinecraftServer server) {
            if (done) {
                return;
            }
            long tick = server.getTickCount() - start;
            for (Step step : steps) {
                if (step.tick() == tick) {
                    step.action().run();
                }
            }
            if (tick >= end) {
                done = true;
            }
        }
    }

    private static void waitScript(MekGameTestHelper helper) {
        if (!Script.done) {
            helper.fail("The script has not finished");
        }
    }

    /// Pôr um cabo com o item, pôr outro ao lado, pôr uma máquina ao lado, quebrá-la, quebrar um cabo, repor um cabo com `setBlock` (o comando
    /// `/setblock` não chama o `setPlacedBy`) e quebrar de novo.
    @GameTest(template = FLOOR, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_basic")
    @TestHolder(description = "Records the events of placing and breaking a cable and a machine next to it.")
    public static void basic(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            Player player = helper.makeMockPlayerLookingAt(new BlockPos(3, 0, 2), Direction.UP);
            ServerLevel level = helper.getLevel();
            BlockPos a = helper.absolutePos(new BlockPos(3, 1, 2));
            BlockPos b = helper.absolutePos(new BlockPos(2, 1, 2));
            BlockPos m = helper.absolutePos(new BlockPos(4, 1, 2));
            List<Step> steps = List.of(
                  new Step(3, () -> {
                      Recorder.mark("place cable A with the item");
                      helper.useOn(new BlockPos(3, 0, 2), new ItemStack(MekanismBlocks.BASIC_UNIVERSAL_CABLE.asItem()), player, Direction.UP);
                  }),
                  new Step(6, () -> {
                      Recorder.mark("place cable B with the item");
                      helper.useOn(new BlockPos(2, 0, 2), new ItemStack(MekanismBlocks.BASIC_UNIVERSAL_CABLE.asItem()), player, Direction.UP);
                  }),
                  new Step(9, () -> {
                      Recorder.mark("place machine with the item");
                      helper.useOn(new BlockPos(4, 0, 2), new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()), player, Direction.UP);
                  }),
                  new Step(12, () -> {
                      Recorder.mark("break machine");
                      level.destroyBlock(m, false);
                  }),
                  new Step(15, () -> {
                      Recorder.mark("set machine with setBlock");
                      level.setBlock(m, machine(), 3);
                  }),
                  new Step(18, () -> {
                      Recorder.mark("break cable B");
                      level.destroyBlock(b, false);
                  }),
                  new Step(21, () -> {
                      Recorder.mark("set cable B with setBlock");
                      level.setBlock(b, cable(), 3);
                  }),
                  new Step(24, () -> {
                      Recorder.mark("break cable A with setBlock air");
                      level.setBlock(a, Blocks.AIR.defaultBlockState(), 3);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("basic", helper, new int[]{0, 0, 0, 7, 3, 5});
                      Script.start(steps, 28);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    /// A linha de cabos atravessa os chunks, com uma máquina no meio dela, na primeira célula do chunk seguinte ao da origem. O chunk da máquina sai
    /// (ticket para o nível de descarga e `processUnloads`), volta 5 ticks depois, e o mesmo com o chunk do cabo da borda.
    @GameTest(template = LINE, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_chunks")
    @TestHolder(description = "Records the events of a chunk with a cable and a machine on its edge leaving and coming back.")
    public static void chunks(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            ServerLevel level = helper.getLevel();
            //O primeiro x relativo que cai num chunk novo, com a máquina nele e o cabo no x anterior (a borda)
            int border = 16 - (helper.absolutePos(BlockPos.ZERO).getX() & 15);
            BlockPos machineAt = new BlockPos(border, 1, 1);
            BlockPos edgeAt = new BlockPos(border - 1, 1, 1);
            ChunkPos machineChunk = ChunkPos.containing(helper.absolutePos(machineAt));
            List<Step> steps = List.of(
                  new Step(1, () -> {
                      Recorder.mark("border x " + border);
                      for (int x = 0; x < LINE_LENGTH; x++) {
                          level.setBlock(helper.absolutePos(new BlockPos(x, 1, 1)), x == border ? machine() : cable(), 3);
                      }
                      Recorder.mark("line built");
                  }),
                  new Step(8, () -> {
                      Recorder.mark("unload the chunk of the machine");
                      ChunkData.update(helper, machineAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                  }),
                  new Step(14, () -> {
                      Recorder.mark("load the chunk of the machine");
                      ChunkData.update(helper, machineAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  }),
                  new Step(34, () -> {
                      Recorder.mark("unload the chunk of the edge cable");
                      ChunkData.update(helper, edgeAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                  }),
                  new Step(40, () -> {
                      Recorder.mark("load the chunk of the edge cable");
                      ChunkData.update(helper, edgeAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  }),
                  new Step(60, () -> {
                      Recorder.mark("set the chunk of the machine inaccessible");
                      absoluteLevel(helper, machineChunk, MekGameTestHelper.INACCESSIBLE_LEVEL);
                  }),
                  new Step(66, () -> {
                      Recorder.mark("set the chunk of the machine accessible");
                      absoluteLevel(helper, machineChunk, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("chunks", helper, new int[]{0, 0, 0, LINE_LENGTH, 3, 3});
                      Script.start(steps, 72);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    /// Como `MekGameTestHelper.setChunkLoadLevel`, mas com o chunk absoluto (o dele soma 8 ao meio do chunk relativo e erra de fileira).
    private static void absoluteLevel(MekGameTestHelper helper, ChunkPos absolute, int newLevel) {
        var distanceManager = helper.getChunkMap().getDistanceManager();
        var holder = distanceManager.getChunk(absolute.pack());
        int old = holder == null ? MekGameTestHelper.UNLOAD_LEVEL : holder.getTicketLevel();
        distanceManager.updateChunkScheduling(absolute.pack(), newLevel, holder, old);
        Recorder.mark("level " + old + " to " + newLevel);
    }

    /// O servidor parado (`/tick freeze`), com passos (`/tick step`): o `runsNormally` que o `onTick` do registro lê, e o que os cabos fazem.
    @GameTest(template = FLOOR, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_frozen")
    @TestHolder(description = "Records the events of a cable while the tick is frozen and stepped.")
    public static void frozen(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            ServerLevel level = helper.getLevel();
            MinecraftServer server = ServerLifecycleHooks.getCurrentServer();
            BlockPos a = helper.absolutePos(new BlockPos(3, 1, 2));
            BlockPos b = helper.absolutePos(new BlockPos(2, 1, 2));
            BlockPos m = helper.absolutePos(new BlockPos(4, 1, 2));
            List<Step> steps = List.of(
                  new Step(1, () -> {
                      level.setBlock(a, cable(), 3);
                      level.setBlock(m, machine(), 3);
                  }),
                  new Step(4, () -> {
                      Recorder.mark("freeze");
                      server.tickRateManager().setFrozen(true);
                  }),
                  new Step(7, () -> {
                      Recorder.mark("place cable B while frozen");
                      level.setBlock(b, cable(), 3);
                  }),
                  new Step(9, () -> {
                      Recorder.mark("step 2");
                      server.tickRateManager().stepGameIfPaused(2);
                  }),
                  new Step(15, () -> {
                      Recorder.mark("break the machine while frozen");
                      level.setBlock(m, Blocks.AIR.defaultBlockState(), 3);
                  }),
                  new Step(18, () -> {
                      Recorder.mark("unfreeze");
                      server.tickRateManager().setFrozen(false);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("frozen", helper, new int[]{0, 0, 0, 7, 3, 5});
                      Script.start(steps, 23);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk(event, "load"));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk(event, "unload"));
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> {
            Recorder.tickEvent("pre", "");
            Script.onPre(event.getServer());
        });
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> {
            Recorder.tickEvent("post", "\"runs\":" + event.getServer().tickRateManager().runsNormally());
            Recorder.write();
        });
    }

    /// Descarrega ou carrega o chunk de `relative` como o `ChunkData` do Mekanism (sem o registro de depuração).
    private static final class ChunkData {

        static void update(MekGameTestHelper helper, BlockPos relative, boolean load, int level) {
            ChunkPos absolute = ChunkPos.containing(helper.absolutePos(relative));
            long packed = absolute.pack();
            var chunkMap = helper.getChunkMap();
            var distanceManager = chunkMap.getDistanceManager();
            var holder = distanceManager.getChunk(packed);
            if (WorldUtils.isChunkLoaded(helper.getLevel(), absolute) == load) {
                Recorder.mark("skipped: chunk already " + (load ? "loaded" : "unloaded"));
                return;
            }
            if (load) {
                holder = distanceManager.updateChunkScheduling(packed, level, holder, MekGameTestHelper.UNLOAD_LEVEL);
                if (holder != null) {
                    distanceManager.chunksToUpdateFutures.add(holder);
                }
            } else if (holder != null) {
                int old = holder.getTicketLevel();
                distanceManager.updateChunkScheduling(packed, level, holder, old);
                chunkMap.processUnloads(mekanism.api.functions.ConstantPredicates.ALWAYS_TRUE);
            }
        }
    }

    /// O block entity do cabo, com uma linha para cada chamada que o jogo faz nele.
    private static class TracedCable extends TileEntityUniversalCable {

        /// Os ouvintes de invalidação de capability de cada vizinho (o level só guarda uma referência fraca).
        private final Map<Direction, ICapabilityInvalidationListener> probes = new EnumMap<>(Direction.class);

        TracedCable(BlockPos pos, BlockState state) {
            super(MekanismBlocks.BASIC_UNIVERSAL_CABLE, pos, state);
        }

        @Override
        public void setLevel(Level level) {
            super.setLevel(level);
            if (level instanceof ServerLevel serverLevel && probes.isEmpty()) {
                //O `AcceptorCache` vigia a capability de cada vizinho com um `BlockCapabilityCache`, que só avisa de novo depois de um `getCapability` (e esse trava o servidor
                // dentro de uma invalidação). O rastro ouve o próprio ouvinte do level, que recebe toda invalidação da posição: um superset do que o cache ouve.
                for (Direction side : Direction.values()) {
                    ICapabilityInvalidationListener listener = () -> {
                        Recorder.cable("cap", getBlockPos(), "\"side\":\"" + side.getName() + "\"");
                        return true;
                    };
                    probes.put(side, listener);
                    serverLevel.registerCapabilityListener(getBlockPos().relative(side), listener);
                }
            }
        }

        @Override
        public void clearRemoved() {
            Recorder.cable("clearRemoved", getBlockPos(), "");
            super.clearRemoved();
        }

        @Override
        public void onLoad() {
            Recorder.cable("onLoad", getBlockPos(), "");
            super.onLoad();
        }

        @Override
        public void onAdded(Level level) {
            Recorder.cable("onAdded", getBlockPos(), "");
            super.onAdded(level);
        }

        @Override
        protected void onUpdateServer(ServerLevel level) {
            Recorder.cable("tick", getBlockPos(), "");
            super.onUpdateServer(level);
        }

        @Override
        public void onNeighborBlockChange(@Nullable Direction side) {
            Recorder.cable("neighbor", getBlockPos(), "\"side\":" + (side == null ? "null" : "\"" + side.getName() + "\""));
            super.onNeighborBlockChange(side);
        }

        @Override
        public void saveAdditional(ValueOutput output) {
            super.saveAdditional(output);
        }

        @Override
        public void onChunkUnloaded() {
            Recorder.cable("onChunkUnloaded", getBlockPos(), "");
            super.onChunkUnloaded();
        }

        @Override
        public void setRemoved() {
            Recorder.cable("setRemoved", getBlockPos(), "");
            super.setRemoved();
        }
    }

    private static class Recorder {

        @Nullable
        private static volatile Recorder current;
        private static boolean installed;
        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final ServerLevel level;
        private final BlockPos origin;
        private final ChunkPos originChunk;
        private final long startTick;
        private final int[] bounds;
        private int written = -1;

        private Recorder(String name, MekGameTestHelper helper, int[] box) {
            this.name = name;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.originChunk = ChunkPos.containing(origin);
            this.startTick = ServerLifecycleHooks.getCurrentServer().getTickCount();
            this.bounds = box.clone();
        }

        static synchronized void installTracedCable() {
            if (installed) {
                return;
            }
            try {
                BlockEntityType<TileEntityUniversalCable> type = MekanismTileEntityTypes.BASIC_UNIVERSAL_CABLE.get();
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                BlockEntityType.BlockEntitySupplier<TileEntityUniversalCable> traced = TracedCable::new;
                factory.set(type, traced);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// `box`: min x, y, z e max x, y, z (exclusivo) da estrutura do cenário em posições relativas.
        static void begin(String name, MekGameTestHelper helper, int[] box) {
            current = new Recorder(name, helper, box);
            Recorder recorder = current;
            recorder.add("origin", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                   + ",\"xInChunk\":" + (recorder.origin.getX() & 15) + ",\"zInChunk\":" + (recorder.origin.getZ() & 15));
            mark("start");
        }

        static synchronized void finish() {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.add("mark", "\"m\":\"finish\"");
                recorder.flush();
                current = null;
            }
        }

        static synchronized void write() {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.flush();
            }
        }

        static void mark(String text) {
            add(recorder -> recorder.add("mark", "\"m\":\"" + text + "\""));
        }

        static void tickEvent(String kind, String fields) {
            add(recorder -> recorder.add(kind, fields));
        }

        static void cable(String kind, BlockPos pos, String fields) {
            add(recorder -> {
                int x = pos.getX() - recorder.origin.getX();
                int y = pos.getY() - recorder.origin.getY();
                int z = pos.getZ() - recorder.origin.getZ();
                int[] b = recorder.bounds;
                //Only the cables of this structure: other tests run at the same time, with cables of their own
                if (x >= b[0] && x < b[3] && y >= b[1] && y < b[4] && z >= b[2] && z < b[5]) {
                    recorder.add(kind, "\"p\":[" + x + "," + y + "," + z + "]" + (fields.isEmpty() ? "" : "," + fields));
                }
            });
        }

        static void ticket(ChunkTicketLevelUpdatedEvent event) {
            add(recorder -> {
                if (event.getLevel() != recorder.level) {
                    return;
                }
                String chunk = recorder.relativeChunk(ChunkPos.unpack(event.getChunkPos()));
                if (chunk != null) {
                    recorder.add("ticket", chunk + ",\"old\":" + event.getOldTicketLevel() + ",\"new\":" + event.getNewTicketLevel());
                }
            });
        }

        static void chunk(ChunkEvent<LevelChunk> event, String kind) {
            add(recorder -> {
                if (event.getLevel() != recorder.level) {
                    return;
                }
                LevelChunk chunk = event.getChunk();
                String relative = recorder.relativeChunk(chunk.getPos());
                if (relative == null) {
                    return;
                }
                String extra = event instanceof ChunkEvent.Load load ? ",\"new\":" + load.isNewChunk() : "";
                recorder.add(kind, relative + extra);
            });
        }

        private static synchronized void add(Consumer<Recorder> action) {
            Recorder recorder = current;
            if (recorder != null) {
                action.accept(recorder);
            }
        }

        private @Nullable String relativeChunk(ChunkPos pos) {
            int x = pos.x() - originChunk.x();
            int z = pos.z() - originChunk.z();
            return x >= 0 && x <= CHUNKS && z >= -1 && z <= 1 ? "\"cx\":" + x + ",\"cz\":" + z : null;
        }

        private void add(String kind, String fields) {
            long tick = ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
            lines.add("{\"t\":" + tick + ",\"e\":\"" + kind + "\"" + (fields.isEmpty() ? "" : "," + fields) + "}");
        }

        private void flush() {
            if (written == lines.size()) {
                return;
            }
            try {
                Path file = Path.of("trace", "transmitter_events_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
                written = lines.size();
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
