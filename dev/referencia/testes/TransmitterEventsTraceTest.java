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
import net.minecraft.core.Holder;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
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

    /// Os comandos que mudam vários blocos de uma vez (`/fill`, `/clone`, `/setblock`) e uma explosão, no cenário do chão de 7 por 5: a fila de cabos em z=2, as máquinas em z=1.
    /// O original manda cada evento dentro da chamada que o causa, no meio da operação (decisão 183 (b)); o rastro mostra a ordem dos eventos de cada comando.
    @GameTest(template = FLOOR, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_ops")
    @TestHolder(description = "Records the events of fill, clone, setblock and an explosion over cables and machines.")
    public static void ops(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            ServerLevel level = helper.getLevel();
            String cable = "mekanism:basic_universal_cable";
            String advanced = "mekanism:advanced_universal_cable";
            String chamber = "mekanism:enrichment_chamber";
            List<Step> steps = List.of(
                  new Step(3, () -> command(helper, "fill the line of cables", "fill", new int[]{1, 1, 2, 5, 1, 2}, cable)),
                  new Step(6, () -> command(helper, "fill the row of machines", "fill", new int[]{1, 1, 1, 5, 1, 1}, chamber)),
                  new Step(9, () -> command(helper, "fill the middle of the line with air", "fill", new int[]{2, 1, 2, 3, 1, 2}, "minecraft:air")),
                  new Step(12, () -> command(helper, "fill the gap with cables again", "fill", new int[]{2, 1, 2, 3, 1, 2}, cable)),
                  new Step(15, () -> {
                      Recorder.mark("build a cable, a stone and a torch on it");
                      level.setBlock(helper.absolutePos(new BlockPos(3, 1, 3)), Blocks.STONE.defaultBlockState(), 3);
                      level.setBlock(helper.absolutePos(new BlockPos(3, 2, 3)), Blocks.TORCH.defaultBlockState(), 3);
                      level.setBlock(helper.absolutePos(new BlockPos(3, 2, 2)), cable(), 3);
                  }),
                  new Step(18, () -> command(helper, "break the stone under the torch next to the cable", "setblock", new int[]{3, 1, 3}, "minecraft:air")),
                  new Step(21, () -> command(helper, "clone the line to z 3", "clone", new int[]{1, 1, 2, 5, 1, 2, 1, 1, 3}, "")),
                  new Step(24, () -> command(helper, "clone the line to z 3 shifted by one with move", "clone", new int[]{1, 1, 3, 5, 1, 3, 2, 1, 3}, "replace move")),
                  new Step(27, () -> command(helper, "setblock stone next to the line", "setblock", new int[]{0, 1, 2}, "minecraft:stone")),
                  new Step(30, () -> command(helper, "fill advanced cables over the line", "fill", new int[]{1, 1, 2, 5, 1, 2}, advanced + " replace " + cable)),
                  new Step(33, () -> command(helper, "fill the machines with air", "fill", new int[]{1, 1, 1, 5, 1, 1}, "minecraft:air")),
                  new Step(36, () -> {
                      Recorder.mark("explosion in the middle of the line");
                      BlockPos at = helper.absolutePos(new BlockPos(3, 1, 2));
                      level.explode(null, at.getX() + 0.5, at.getY() + 0.5, at.getZ() + 0.5, 2.0F, Level.ExplosionInteraction.TNT);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("ops", helper, new int[]{0, 0, 0, 7, 3, 5});
                      Script.start(steps, 42);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    /// Roda `/<name> <posições absolutas> <extra>` como o servidor, no nível do cenário. `box` são as posições relativas à estrutura, na ordem do comando.
    private static void command(MekGameTestHelper helper, String mark, String name, int[] box, String extra) {
        Recorder.mark(mark);
        StringBuilder text = new StringBuilder(name);
        for (int i = 0; i + 2 < box.length; i += 3) {
            BlockPos pos = helper.absolutePos(new BlockPos(box[i], box[i + 1], box[i + 2]));
            text.append(' ').append(pos.getX()).append(' ').append(pos.getY()).append(' ').append(pos.getZ());
        }
        if (!extra.isEmpty()) {
            text.append(' ').append(extra);
        }
        MinecraftServer server = ServerLifecycleHooks.getCurrentServer();
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack().withLevel(helper.getLevel()), text.toString());
    }

    /// Os chunks da máquina e do cabo da borda saem e voltam no mesmo tick (a descarga e a carga uma depois da outra, e a carga e a descarga), e a máquina fica inacessível e acessível.
    @GameTest(template = LINE, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_chunks_same_tick")
    @TestHolder(description = "Records the events of chunks that unload and load again in the same tick.")
    public static void chunks_same_tick(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            ServerLevel level = helper.getLevel();
            int border = 16 - (helper.absolutePos(BlockPos.ZERO).getX() & 15);
            BlockPos machineAt = new BlockPos(border, 1, 1);
            BlockPos edgeAt = new BlockPos(border - 1, 1, 1);
            List<Step> steps = List.of(
                  new Step(1, () -> {
                      Recorder.mark("border x " + border);
                      for (int x = 0; x < LINE_LENGTH; x++) {
                          level.setBlock(helper.absolutePos(new BlockPos(x, 1, 1)), x == border ? machine() : cable(), 3);
                      }
                      Recorder.mark("line built");
                  }),
                  new Step(8, () -> {
                      Recorder.mark("unload and load the chunk of the machine in the same tick");
                      ChunkData.update(helper, machineAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                      ChunkData.update(helper, machineAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  }),
                  new Step(20, () -> {
                      Recorder.mark("unload the chunk of the edge cable and the machine, load both, in the same tick");
                      ChunkData.update(helper, edgeAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                      ChunkData.update(helper, machineAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                      ChunkData.update(helper, machineAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                      ChunkData.update(helper, edgeAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("chunks_same_tick", helper, new int[]{0, 0, 0, LINE_LENGTH, 3, 3});
                      Script.start(steps, 32);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    /// Dois chunks com cabos e baús postos fora de ordem (e alguns tirados e postos de novo), que saem um de cada vez: a ordem em que o original chama o `onChunkUnloaded` e o `setRemoved`
    /// de cada cabo é a do mapa de block entities do chunk (todos os tipos, na ordem em que entraram), e o port só vê os de mod.
    @GameTest(template = LINE, timeoutTicks = TIMEOUT, batch = "trace_transmitter_events_chunks_mixed")
    @TestHolder(description = "Records the order in which a chunk with cables and chests in a scrambled order unloads its block entities.")
    public static void chunks_mixed(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            ServerLevel level = helper.getLevel();
            int border = 16 - (helper.absolutePos(BlockPos.ZERO).getX() & 15);
            BlockPos firstAt = new BlockPos(border, 1, 1);
            BlockPos secondAt = new BlockPos(border + 16, 1, 1);
            List<Step> steps = List.of(
                  new Step(1, () -> {
                      Recorder.mark("border x " + border);
                      mixed(helper, level, border, 14, 7, 4);
                      mixed(helper, level, border + 16, 36, 7, 5);
                      Recorder.mark("chunks built");
                  }),
                  new Step(6, () -> {
                      Recorder.mark("take three out of the second chunk and put two back");
                      for (int k : new int[]{14, 21, 35}) {
                          level.setBlock(helper.absolutePos(new BlockPos(border + 16 + k % 16, 1, k / 16)), Blocks.AIR.defaultBlockState(), 3);
                      }
                      for (int k : new int[]{35, 14}) {
                          level.setBlock(helper.absolutePos(new BlockPos(border + 16 + k % 16, 1, k / 16)), cable(), 3);
                      }
                  }),
                  new Step(12, () -> {
                      Recorder.mark("unload the first chunk");
                      ChunkData.update(helper, firstAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                  }),
                  new Step(18, () -> {
                      Recorder.mark("load the first chunk");
                      ChunkData.update(helper, firstAt, true, ChunkMap.MAX_VIEW_DISTANCE - 2);
                  }),
                  new Step(30, () -> {
                      Recorder.mark("unload the second chunk");
                      ChunkData.update(helper, secondAt, false, MekGameTestHelper.UNLOAD_LEVEL);
                  })
            );
            helper.startSequence()
                  .thenExecute(() -> {
                      Recorder.begin("chunks_mixed", helper, new int[]{0, 0, 0, LINE_LENGTH, 3, 3});
                      Script.start(steps, 36);
                  })
                  .thenWaitUntil(() -> waitScript(helper))
                  .thenExecute(Recorder::finish)
                  .thenSucceed();
        });
    }

    /// `count` posições do chunk que começa em `fromX`, fora de ordem (`step` é primo com 48), cada `chestEvery`-ésima um baú e as outras cabos.
    private static void mixed(MekGameTestHelper helper, ServerLevel level, int fromX, int count, int step, int chestEvery) {
        for (int i = 0; i < count; i++) {
            int k = (i * step) % 48;
            BlockState state = i % chestEvery == chestEvery - 1 ? Blocks.CHEST.defaultBlockState() : cable();
            level.setBlock(helper.absolutePos(new BlockPos(fromX + k % 16, 1, k / 16)), state, 3);
        }
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

        TracedCable(Holder<Block> block, BlockPos pos, BlockState state) {
            super(block, pos, state);
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
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                // O cabo básico e o avançado: os cenários trocam um pelo outro.
                BlockEntityType<TileEntityUniversalCable> basic = MekanismTileEntityTypes.BASIC_UNIVERSAL_CABLE.get();
                BlockEntityType.BlockEntitySupplier<TileEntityUniversalCable> tracedBasic = (pos, state) -> new TracedCable(MekanismBlocks.BASIC_UNIVERSAL_CABLE, pos, state);
                factory.set(basic, tracedBasic);
                BlockEntityType<TileEntityUniversalCable> advanced = MekanismTileEntityTypes.ADVANCED_UNIVERSAL_CABLE.get();
                BlockEntityType.BlockEntitySupplier<TileEntityUniversalCable> tracedAdvanced = (pos, state) -> new TracedCable(MekanismBlocks.ADVANCED_UNIVERSAL_CABLE, pos, state);
                factory.set(advanced, tracedAdvanced);
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
