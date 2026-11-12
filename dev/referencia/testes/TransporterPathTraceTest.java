package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongList;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.text.EnumColor;
import mekanism.common.Mekanism;
import mekanism.common.content.network.InventoryNetwork;
import mekanism.common.content.network.InventoryNetwork.AcceptorData;
import mekanism.common.content.network.transmitter.DiversionTransporter;
import mekanism.common.content.network.transmitter.DiversionTransporter.DiversionControl;
import mekanism.common.content.network.transmitter.LogisticalTransporter;
import mekanism.common.content.network.transmitter.LogisticalTransporterBase;
import mekanism.common.content.network.transmitter.RestrictiveTransporter;
import mekanism.common.content.transporter.PathfinderCache;
import mekanism.common.content.transporter.TransporterManager;
import mekanism.common.content.transporter.TransporterPathfinder.Destination;
import mekanism.common.content.transporter.TransporterStack;
import mekanism.common.lib.SidedBlockPos;
import mekanism.common.lib.inventory.IAdvancedTransportEjector;
import mekanism.common.lib.inventory.TransitRequest;
import mekanism.common.lib.inventory.TransitRequest.TransitResponse;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tests.util.StructureBuilderUtils;
import mekanism.common.tile.transmitter.TileEntityLogisticalTransporterBase;
import mekanism.common.util.EnumUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueOutput;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.TransactionContext;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava, para uma pilha posta num transportador logístico, para onde o original a manda e por que caminho, um `.jsonl` por cenário em
/// `trace/transporter_path_<cenário>.jsonl` (frente `transporte-caminho`, Fase 3). Cada cenário monta uma rede de transportadores e baús e roda uma lista
/// de consultas direto nas funções públicas do original, sem esperar o tick:
///
/// - `base`: `TransporterStack.recalculatePath` (o ejetor comum), `rr`: `recalculateRRPath` (o ejetor com round robin), `idle`: `calculateIdle`;
/// - antes de cada consulta, `InventoryNetwork.calculateAcceptors` (os aceitadores na ordem da tabela) e `TransporterPathfinder.getPaths` (por reflexão: a
///   lista de destinos já ordenada, com o custo de cada um), e depois o resultado: a resposta, o caminho, o `pathType`, o `idleDir`, a casa e o NBT da pilha.
///
/// As posições são relativas à origem da estrutura, mas as absolutas valem para a ordem das tabelas do fastutil (`mix(long)`), então o `setup` leva a origem.
@ForEachTest(groups = "trace_transporter_path")
public class TransporterPathTraceTest {

    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final int TIMEOUT = 10 * SharedConstants.TICKS_PER_SECOND;

    private static BlockState basic() {
        return MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState();
    }

    private static BlockState ultimate() {
        return MekanismBlocks.ULTIMATE_LOGISTICAL_TRANSPORTER.defaultState();
    }

    private static BlockState chestState() {
        return Blocks.CHEST.defaultBlockState();
    }

    /// O que o cenário montou, para o `setup` saber onde olhar.
    private static final class Layout {

        final List<int[]> transporters = new ArrayList<>();
        final List<int[]> chests = new ArrayList<>();

        Layout t(StructureTemplateBuilder builder, int x, int y, int z, BlockState state, @Nullable CompoundTag tag) {
            transporters.add(new int[]{x, y, z});
            builder.set(x, y, z, state, tag);
            return this;
        }

        Layout row(StructureTemplateBuilder builder, int x1, int x2, int y, int z, BlockState state) {
            for (int x = x1; x <= x2; x++) {
                t(builder, x, y, z, state, null);
            }
            return this;
        }

        Layout col(StructureTemplateBuilder builder, int x, int y, int z1, int z2, BlockState state) {
            for (int z = z1; z <= z2; z++) {
                t(builder, x, y, z, state, null);
            }
            return this;
        }

        Layout chest(StructureTemplateBuilder builder, int x, int y, int z) {
            chests.add(new int[]{x, y, z});
            builder.set(x, y, z, chestState());
            return this;
        }
    }

    private static CompoundTag colored(EnumColor color) {
        return StructureBuilderUtils.configured(color, null, ConnectionType.NORMAL);
    }

    private static Method getPaths;
    private static Field destinationScore;

    static {
        try {
            getPaths = mekanism.common.content.transporter.TransporterPathfinder.class.getDeclaredMethod("getPaths", LogisticalTransporterBase.class,
                  TransporterStack.class, TransitRequest.class, int.class, TransactionContext.class);
            getPaths.setAccessible(true);
            destinationScore = Destination.class.getDeclaredField("score");
            destinationScore.setAccessible(true);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /// O ejetor de round robin: só guarda o alvo.
    private static final class Ejector implements IAdvancedTransportEjector {

        @Nullable
        SidedBlockPos target;

        @Nullable
        @Override
        public SidedBlockPos getRoundRobinTarget() {
            return target;
        }

        @Override
        public void setRoundRobinTarget(@Nullable SidedBlockPos target) {
            this.target = target;
        }

        @Override
        public boolean getRoundRobin() {
            return true;
        }

        @Override
        public void toggleRoundRobin() {
        }

        @Override
        public boolean canSendHome(Level level, ItemResource itemType, int amount, @Nullable TransactionContext transaction) {
            return false;
        }

        @Override
        public TransitResponse sendHome(Level level, TransitRequest request, TransactionContext transaction) {
            return TransitResponse.EMPTY;
        }
    }

    /// O gravador de um cenário.
    private static final class Rec {

        final String name;
        final MekGameTestHelper helper;
        final ServerLevel level;
        final BlockPos origin;
        final Layout layout;
        final List<String> lines = new ArrayList<>();
        int queryIndex;

        Rec(String name, MekGameTestHelper helper, Layout layout) {
            this.name = name;
            this.helper = helper;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.layout = layout;
            TransporterManager.reset();
            PathfinderCache.reset();
        }

        String rel(BlockPos pos) {
            return "[" + (pos.getX() - origin.getX()) + "," + (pos.getY() - origin.getY()) + "," + (pos.getZ() - origin.getZ()) + "]";
        }

        String rel(long packed) {
            return packed == Long.MAX_VALUE ? "null" : rel(BlockPos.of(packed));
        }

        String rel(int[] p) {
            return "[" + p[0] + "," + p[1] + "," + p[2] + "]";
        }

        String path(LongList list) {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < list.size(); i++) {
                out.append(i == 0 ? "" : ",").append(rel(list.getLong(i)));
            }
            return out.append("]").toString();
        }

        static String dir(@Nullable Direction side) {
            return side == null ? "null" : "\"" + side.getName() + "\"";
        }

        static String color(@Nullable EnumColor color) {
            return color == null ? "null" : "\"" + color.getSerializedName() + "\"";
        }

        static String item(Item item) {
            return "\"" + BuiltInRegistries.ITEM.getKey(item) + "\"";
        }

        BlockPos abs(int x, int y, int z) {
            return origin.offset(x, y, z);
        }

        LogisticalTransporterBase transporter(int x, int y, int z) {
            return helper.getBlockEntity(x, y, z, TileEntityLogisticalTransporterBase.class).getTransmitter();
        }

        ChestBlockEntity chest(int x, int y, int z) {
            return helper.getBlockEntity(x, y, z, ChestBlockEntity.class);
        }

        void add(String text) {
            lines.add(text);
        }

        String chestsJson() {
            StringBuilder out = new StringBuilder("[");
            boolean first = true;
            for (int[] p : layout.chests) {
                ChestBlockEntity chest = chest(p[0], p[1], p[2]);
                StringBuilder slots = new StringBuilder();
                for (int i = 0; i < chest.getContainerSize(); i++) {
                    ItemStack stack = chest.getItem(i);
                    if (!stack.isEmpty()) {
                        slots.append(slots.isEmpty() ? "" : ",").append("[").append(i).append(",").append(item(stack.getItem())).append(",").append(stack.getCount()).append("]");
                    }
                }
                out.append(first ? "" : ",").append("{\"p\":").append(rel(p)).append(",\"slots\":[").append(slots).append("]}");
                first = false;
            }
            return out.append("]").toString();
        }

        /// O estado de cada transportador e dos baús, como o original os vê agora.
        void setup() {
            StringBuilder ts = new StringBuilder();
            for (int[] p : layout.transporters) {
                LogisticalTransporterBase t = transporter(p[0], p[1], p[2]);
                String kind = t instanceof DiversionTransporter ? "diversion" : t instanceof RestrictiveTransporter ? "restrictive" : "logistical";
                StringBuilder raw = new StringBuilder();
                StringBuilder types = new StringBuilder();
                StringBuilder connect = new StringBuilder();
                StringBuilder emit = new StringBuilder();
                StringBuilder receive = new StringBuilder();
                StringBuilder modes = new StringBuilder();
                for (Direction side : EnumUtils.DIRECTIONS) {
                    String sep = side.ordinal() == 0 ? "" : ",";
                    raw.append(sep).append(t.getConnectionTypeRaw(side).ordinal());
                    types.append(sep).append(t.getConnectionType(side).ordinal());
                    connect.append(sep).append(t.canConnect(side));
                    emit.append(sep).append(t.canEmitTo(side));
                    receive.append(sep).append(t.canReceiveFrom(side));
                    if (t instanceof DiversionTransporter diversion) {
                        modes.append(sep).append(diversion.modes[side.ordinal()].ordinal());
                    }
                }
                InventoryNetwork network = t.getTransmitterNetwork();
                ts.append(ts.isEmpty() ? "" : ",").append("{\"p\":").append(rel(p)).append(",\"kind\":\"").append(kind).append("\",\"tier\":").append(t.tier.ordinal())
                      .append(",\"color\":").append(color(t.getColor())).append(",\"cost\":").append(t.getCost())
                      .append(",\"raw\":[").append(raw).append("],\"types\":[").append(types).append("],\"currentTx\":").append(t.currentTransmitterConnections)
                      .append(",\"currentAll\":").append(t.getAllCurrentConnections())
                      .append(",\"connect\":[").append(connect).append("],\"emit\":[").append(emit).append("],\"receive\":[").append(receive).append("]")
                      .append(kind.equals("diversion") ? ",\"modes\":[" + modes + "]" : "")
                      .append(",\"netHash\":").append(network == null ? "null" : network.getUUID().hashCode())
                      .append(",\"netSize\":").append(network == null ? 0 : network.transmittersSize())
                      .append(",\"netAcceptors\":").append(network == null ? 0 : network.getAcceptorCount()).append("}");
            }
            add("{\"e\":\"setup\",\"name\":\"" + name + "\",\"x\":" + origin.getX() + ",\"y\":" + origin.getY() + ",\"z\":" + origin.getZ()
                + ",\"transporters\":[" + ts + "],\"chests\":" + chestsJson() + "}");
        }

        void mark(String text) {
            add("{\"e\":\"mark\",\"m\":\"" + text + "\"}");
        }

        /// Troca o conteúdo de um baú: `slots` são (slot, item, quantidade).
        void fill(int x, int y, int z, int slots, Item item, int count) {
            ChestBlockEntity chest = chest(x, y, z);
            for (int i = 0; i < slots; i++) {
                chest.setItem(i, new ItemStack(item, count));
            }
        }

        void clear(int x, int y, int z) {
            chest(x, y, z).clearContent();
        }

        void setColor(int x, int y, int z, @Nullable EnumColor color) {
            LogisticalTransporterBase t = transporter(x, y, z);
            ((LogisticalTransporter) t).setColor(color);
            InventoryNetwork network = t.getTransmitterNetwork();
            if (network != null) {
                PathfinderCache.onChanged(network);
            }
            mark("color " + rel(new int[]{x, y, z}) + " " + (color == null ? "none" : color.getSerializedName()));
        }

        String acceptorList(TransitRequest request, TransporterStack stack, LogisticalTransporterBase start) {
            InventoryNetwork network = start.getTransmitterNetwork();
            if (network == null) {
                return "[]";
            }
            List<AcceptorData> data = network.calculateAcceptors(request, stack, new Long2ObjectOpenHashMap<>(), start, null);
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < data.size(); i++) {
                AcceptorData acceptor = data.get(i);
                StringBuilder sides = new StringBuilder();
                for (Direction side : acceptor.getSides()) {
                    sides.append(sides.isEmpty() ? "" : ",").append("\"").append(side.getName()).append("\"");
                }
                TransitResponse response = acceptor.getResponse();
                out.append(i == 0 ? "" : ",").append("{\"p\":").append(rel(acceptor.getLocation())).append(",\"sides\":[").append(sides).append("],\"item\":")
                      .append(item(response.itemType().toStack(1).getItem())).append(",\"amount\":").append(response.sendingAmount()).append("}");
            }
            return out.append("]").toString();
        }

        @SuppressWarnings("unchecked")
        String destinationList(TransitRequest request, TransporterStack stack, LogisticalTransporterBase start, int min) {
            try {
                List<Destination> destinations = (List<Destination>) getPaths.invoke(null, start, stack, request, min, null);
                StringBuilder out = new StringBuilder("[");
                for (int i = 0; i < destinations.size(); i++) {
                    Destination destination = destinations.get(i);
                    out.append(i == 0 ? "" : ",").append("{\"path\":").append(path(destination.getPath())).append(",\"score\":").append(destinationScore.getDouble(destination))
                          .append(",\"amount\":").append(destination.getResponseOrEmpty().sendingAmount()).append("}");
                }
                return out.append("]").toString();
            } catch (IllegalAccessException | InvocationTargetException e) {
                throw new IllegalStateException(e);
            }
        }

        String nbt(TransporterStack stack) {
            try (ProblemReporter.ScopedCollector reporter = new ProblemReporter.ScopedCollector(Mekanism.logger)) {
                TagValueOutput output = TagValueOutput.createWithContext(reporter, ServerLifecycleHooks.getCurrentServer().registryAccess());
                stack.write(output);
                CompoundTag tag = output.buildResult();
                return "\"" + tag.toString().replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
            }
        }

        String stackState(TransporterStack stack) {
            return "\"pathType\":\"" + stack.getPathType().getSerializedName() + "\",\"path\":" + path(stack.getPath()) + ",\"idleDir\":" + dir(stack.idleDir)
                   + ",\"home\":" + rel(stack.homeLocation) + ",\"original\":" + rel(stack.originalLocation) + ",\"progress\":" + stack.progress
                   + ",\"hasPath\":" + stack.hasPath() + ",\"sideOfDest\":" + dir(stack.hasPath() ? stack.getSideOfDest() : null) + ",\"nbt\":" + nbt(stack);
        }

        String ejectorState(@Nullable Ejector ejector) {
            if (ejector == null) {
                return "null";
            }
            SidedBlockPos target = ejector.target;
            return target == null ? "null" : "{\"p\":" + rel(target.pos()) + ",\"side\":" + dir(target.side()) + "}";
        }

        /// `base` ou `rr` (com `ejector`): a pilha nasce na casa `home` (um baú ao lado do transportador) com a cor dada.
        void query(String note, int[] start, @Nullable EnumColor color, Item item, int amount, int min, int[] home, boolean keep, @Nullable Ejector ejector) {
            LogisticalTransporterBase transporter = transporter(start[0], start[1], start[2]);
            TransporterStack stack = new TransporterStack();
            long homePos = abs(home[0], home[1], home[2]).asLong();
            stack.originalLocation = homePos;
            stack.homeLocation = homePos;
            stack.color = color;
            TransitRequest request = TransitRequest.simple(ItemResource.of(new ItemStack(item)), amount);
            int index = queryIndex++;
            String chests = chestsJson();
            String acceptors = acceptorList(request, stack, transporter);
            String destinations = destinationList(request, stack, transporter, min);
            String before = ejectorState(ejector);
            TransitResponse response = ejector == null ? stack.recalculatePath(level, request, transporter, min, null)
                                                       : stack.recalculateRRPath(level, request, ejector, transporter, min, null);
            if (!response.isEmpty()) {
                stack.setStack(response.itemType(), response.sendingAmount());
            }
            add("{\"e\":\"query\",\"q\":" + index + ",\"note\":\"" + note + "\",\"kind\":\"" + (ejector == null ? "base" : "rr") + "\",\"start\":" + rel(start)
                + ",\"color\":" + color(color) + ",\"item\":" + item(item) + ",\"amount\":" + amount + ",\"min\":" + min + ",\"homePos\":" + rel(home)
                + ",\"chests\":" + chests + ",\"acceptors\":" + acceptors + ",\"destinations\":" + destinations
                + ",\"rrBefore\":" + before + ",\"rrAfter\":" + ejectorState(ejector)
                + ",\"empty\":" + response.isEmpty() + ",\"respItem\":" + (response.isEmpty() ? "null" : item(response.itemType().toStack(1).getItem()))
                + ",\"respAmount\":" + response.sendingAmount() + "," + stackState(stack) + "}");
            if (!keep) {
                TransporterManager.remove(level, stack, null);
            }
        }

        /// `calculateIdle`: a pilha já tem o item (e a casa, o `idleDir` e o progresso dados). Devolve o transportador onde o caminho termina, se for um.
        @Nullable
        int[] idle(String note, int[] start, @Nullable EnumColor color, Item item, int amount, @Nullable int[] home, @Nullable Direction idleDir, int progress,
              int[] original, @Nullable TransporterStack previous) {
            LogisticalTransporterBase transporter = transporter(start[0], start[1], start[2]);
            TransporterStack stack = previous == null ? new TransporterStack() : previous;
            if (previous == null) {
                stack.setStack(ItemResource.of(new ItemStack(item)), amount);
                stack.color = color;
                stack.homeLocation = home == null ? Long.MAX_VALUE : abs(home[0], home[1], home[2]).asLong();
                stack.idleDir = idleDir;
                stack.progress = progress;
                stack.originalLocation = abs(original[0], original[1], original[2]).asLong();
            }
            int index = queryIndex++;
            String chests = chestsJson();
            boolean found = stack.calculateIdle(level, transporter, null);
            add("{\"e\":\"idle\",\"q\":" + index + ",\"note\":\"" + note + "\",\"start\":" + rel(start) + ",\"color\":" + color(color) + ",\"item\":" + item(item)
                + ",\"amount\":" + amount + ",\"chests\":" + chests + ",\"found\":" + found + "," + stackState(stack) + "}");
            lastIdle = stack;
            lastFound = found;
            TransporterManager.remove(level, stack, null);
            return null;
        }

        @Nullable
        TransporterStack lastIdle;
        boolean lastFound;

        void finish() {
            add("{\"e\":\"mark\",\"m\":\"finish\"}");
            write();
        }

        void write() {
            try {
                Path file = Path.of("trace", "transporter_path_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    private static final Item STONE = Items.STONE;

    /// Roda o roteiro e grava o arquivo mesmo se ele falhar, com o erro na última linha.
    private static void run(Rec rec, Runnable script) {
        try {
            rec.setup();
            script.run();
            rec.finish();
        } catch (RuntimeException | Error e) {
            rec.add("{\"e\":\"error\",\"m\":\"" + String.valueOf(e).replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", " ") + "\"}");
            rec.write();
            throw e;
        }
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Um baú no fim de uma linha
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_line")
    @TestHolder(description = "Records the destination and path of a stack put in a line of logistical transporters with a chest at its end.")
    public static void line(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 5, 0, 0, basic()).chest(builder, 6, 0, 0);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("line", helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                rec.query("from the first transporter", new int[]{1, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the middle", new int[]{3, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the last, next to the chest", new int[]{5, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("min above the amount", new int[]{1, 0, 0}, null, STONE, 10, 11, home, false, null);
                rec.query("min equal to the amount", new int[]{1, 0, 0}, null, STONE, 10, 10, home, false, null);
                rec.query("the end chest is the home, the first is the destination", new int[]{5, 0, 0}, null, STONE, 10, 0, new int[]{6, 0, 0}, false, null);
                rec.query("a colored stack", new int[]{1, 0, 0}, EnumColor.RED, STONE, 10, 0, home, false, null);
                rec.idle("idle, the home chest is the destination", new int[]{3, 0, 0}, null, STONE, 10, home, null, 0, new int[]{1, 0, 0}, null);
            });
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Dois baús a distâncias diferentes
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static void twoChests(final DynamicTest test, String name, boolean ultimateBranch) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(6, 1, 5);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 4, 0, 0, basic()).chest(builder, 5, 0, 0)
                  .col(builder, 2, 0, 1, 3, ultimateBranch ? ultimate() : basic()).chest(builder, 2, 0, 4);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec(name, helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                rec.query("from the first", new int[]{1, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the near chest end", new int[]{4, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the branch", new int[]{2, 0, 2}, null, STONE, 10, 0, home, false, null);
                rec.query("the near chest is the home", new int[]{4, 0, 0}, null, STONE, 10, 0, new int[]{5, 0, 0}, false, null);
                rec.query("the far chest is the home", new int[]{2, 0, 3}, null, STONE, 10, 0, new int[]{2, 0, 4}, false, null);
            });
        }).thenSucceed());
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_two_ultimate")
    @TestHolder(description = "Records the choice between two chests when the way to the far one is made of cheaper transporters.")
    public static void twoUltimate(final DynamicTest test) {
        twoChests(test, "two_ultimate", true);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_two_basic")
    @TestHolder(description = "Records the choice between two chests at different distances with the same kind of transporter.")
    public static void twoBasic(final DynamicTest test) {
        twoChests(test, "two_basic", false);
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Um baú cheio e outro com espaço
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_full")
    @TestHolder(description = "Records the destination when the near chest is full, nearly full, or has stacks in flight to it.")
    public static void full(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(6, 1, 5);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 4, 0, 0, basic()).chest(builder, 5, 0, 0)
                  .col(builder, 2, 0, 1, 3, basic()).chest(builder, 2, 0, 4);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("full", helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                int[] start = {1, 0, 0};
                rec.query("both empty", start, null, STONE, 10, 0, home, false, null);
                rec.fill(5, 0, 0, 27, STONE, 64);
                rec.mark("the near chest is full of stone");
                rec.query("near full", start, null, STONE, 10, 0, home, false, null);
                rec.query("near full, another item", start, null, Items.APPLE, 10, 0, home, false, null);
                rec.clear(5, 0, 0);
                rec.fill(5, 0, 0, 26, STONE, 64);
                rec.chest(5, 0, 0).setItem(26, new ItemStack(STONE, 60));
                rec.mark("the near chest has room for 4 stone");
                rec.query("near has 4 free", start, null, STONE, 64, 0, home, false, null);
                rec.query("near has 4 free, min 4", start, null, STONE, 64, 4, home, false, null);
                rec.query("near has 4 free, min 5", start, null, STONE, 64, 5, home, false, null);
                rec.fill(2, 0, 4, 27, STONE, 64);
                rec.mark("the far chest is full too");
                rec.query("both full of stone, min 5", start, null, STONE, 64, 5, home, false, null);
                rec.query("both full of stone, an apple", start, null, Items.APPLE, 64, 0, home, false, null);
                rec.clear(5, 0, 0);
                rec.clear(2, 0, 4);
                rec.fill(5, 0, 0, 26, STONE, 64);
                rec.mark("the near chest has one free slot, stacks stay in flight");
                rec.query("first in flight", start, null, STONE, 64, 0, home, true, null);
                rec.query("second in flight, the slot is taken", start, null, STONE, 64, 0, home, true, null);
                rec.query("third in flight", start, null, STONE, 64, 0, home, true, null);
                rec.query("an apple, the slot is taken", start, null, Items.APPLE, 10, 0, home, true, null);
            });
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O round robin de um ejetor com vários destinos
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_round_robin")
    @TestHolder(description = "Records the round robin target of an ejector over several requests, with a destination filling up in the middle.")
    public static void roundRobin(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 1, 5);
            //The junction at (2, 2), the source chest below it, and an arm to the north, the west and (longer) the east
            layout.t(builder, 2, 0, 2, basic(), null).t(builder, 2, 0, 3, basic(), null)
                  .chest(builder, 2, 0, 4)
                  .t(builder, 2, 0, 1, basic(), null).chest(builder, 2, 0, 0)
                  .t(builder, 1, 0, 2, basic(), null).chest(builder, 0, 0, 2)
                  .row(builder, 3, 4, 0, 2, basic()).chest(builder, 5, 0, 2);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("round_robin", helper, layout);
            run(rec, () -> {
                int[] home = {2, 0, 4};
                int[] start = {2, 0, 3};
                Ejector ejector = new Ejector();
                for (int i = 0; i < 7; i++) {
                    rec.query("round robin request " + i, start, null, STONE, 10, 0, home, true, ejector);
                }
                rec.fill(2, 0, 0, 27, Items.DIRT, 64);
                rec.mark("the north chest is full of dirt");
                for (int i = 0; i < 4; i++) {
                    rec.query("after the north chest filled " + i, start, null, STONE, 10, 0, home, true, ejector);
                }
                rec.query("a single request with a fresh ejector", start, null, STONE, 10, 0, home, true, new Ejector());
            });
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Transportadores coloridos
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_colors")
    @TestHolder(description = "Records which stacks pass a colored transporter, with a cached path that stops being valid.")
    public static void colors(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(6, 1, 2);
            layout.chest(builder, 0, 0, 0).t(builder, 1, 0, 0, basic(), null).t(builder, 2, 0, 0, basic(), colored(EnumColor.RED))
                  .row(builder, 3, 4, 0, 0, basic()).chest(builder, 5, 0, 0)
                  .row(builder, 1, 3, 0, 1, basic());
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("colors", helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                int[] start = {1, 0, 0};
                rec.query("red, through the red transporter", start, EnumColor.RED, STONE, 10, 0, home, false, null);
                rec.query("no color, the red one is avoided (the cache is stale)", start, null, STONE, 10, 0, home, false, null);
                rec.query("blue, the red one is avoided", start, EnumColor.BLUE, STONE, 10, 0, home, false, null);
                rec.query("no color again, from the cache", start, null, STONE, 10, 0, home, false, null);
                rec.setColor(3, 0, 0, EnumColor.BLUE);
                rec.setColor(3, 0, 1, EnumColor.BLUE);
                rec.query("no color, blue ones in the way", start, null, STONE, 10, 0, home, false, null);
                rec.query("red, a blue one in the way", start, EnumColor.RED, STONE, 10, 0, home, false, null);
                rec.query("blue, the red one is in the way", start, EnumColor.BLUE, STONE, 10, 0, home, false, null);
                rec.setColor(2, 0, 0, null);
                rec.query("blue, the red one is gone", start, EnumColor.BLUE, STONE, 10, 0, home, false, null);
                rec.idle("idle, colored stack, home is reachable", new int[]{2, 0, 1}, EnumColor.BLUE, STONE, 10, home, null, 0, new int[]{1, 0, 0}, null);
            });
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Restritivo e desviador
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static void restrictive(final DynamicTest test, String name, boolean ladder) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 1, 2);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 2, 0, 0, basic())
                  .t(builder, 3, 0, 0, MekanismBlocks.RESTRICTIVE_TRANSPORTER.defaultState(), null)
                  .row(builder, 4, 5, 0, 0, basic()).chest(builder, 6, 0, 0);
            if (ladder) {
                layout.row(builder, 2, 4, 0, 1, basic());
            }
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec(name, helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                rec.query("from the first", new int[]{1, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the one before the restrictive", new int[]{2, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the restrictive", new int[]{3, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("the end chest is the home", new int[]{3, 0, 0}, null, STONE, 10, 0, new int[]{6, 0, 0}, false, null);
            });
        }).thenSucceed());
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_restrictive_only")
    @TestHolder(description = "Records the path through a restrictive transporter that is the only way.")
    public static void restrictiveOnly(final DynamicTest test) {
        restrictive(test, "restrictive_only", false);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_restrictive_ladder")
    @TestHolder(description = "Records the path that goes around a restrictive transporter through a second row of transporters.")
    public static void restrictiveLadder(final DynamicTest test) {
        restrictive(test, "restrictive_ladder", true);
    }

    private static void diversion(final DynamicTest test, String name, DiversionControl mode, boolean powered) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 2, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 2, 0, 0, basic())
                  .t(builder, 3, 0, 0, MekanismBlocks.DIVERSION_TRANSPORTER.defaultState(),
                        StructureBuilderUtils.diversionModes(null, DiversionControl.DISABLED, DiversionControl.DISABLED, DiversionControl.DISABLED, DiversionControl.DISABLED,
                              DiversionControl.DISABLED, mode))
                  .row(builder, 4, 5, 0, 0, basic()).chest(builder, 6, 0, 0);
            //A redstone block over the diverter, or far from every transporter
            builder.set(powered ? 3 : 0, 1, 0, Blocks.REDSTONE_BLOCK.defaultBlockState());
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec(name, helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                rec.query("from the first", new int[]{1, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("from the diverter", new int[]{3, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.query("the end chest is the home", new int[]{3, 0, 0}, null, STONE, 10, 0, new int[]{6, 0, 0}, false, null);
                rec.query("from after the diverter", new int[]{4, 0, 0}, null, STONE, 10, 0, home, false, null);
            });
        }).thenSucceed());
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_diversion_high_off")
    @TestHolder(description = "Records a diversion transporter with the east side on HIGH and no signal.")
    public static void diversionHighOff(final DynamicTest test) {
        diversion(test, "diversion_high_off", DiversionControl.HIGH, false);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_diversion_high_on")
    @TestHolder(description = "Records a diversion transporter with the east side on HIGH and a signal.")
    public static void diversionHighOn(final DynamicTest test) {
        diversion(test, "diversion_high_on", DiversionControl.HIGH, true);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_diversion_low_off")
    @TestHolder(description = "Records a diversion transporter with the east side on LOW and no signal.")
    public static void diversionLowOff(final DynamicTest test) {
        diversion(test, "diversion_low_off", DiversionControl.LOW, false);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_diversion_low_on")
    @TestHolder(description = "Records a diversion transporter with the east side on LOW and a signal.")
    public static void diversionLowOn(final DynamicTest test) {
        diversion(test, "diversion_low_on", DiversionControl.LOW, true);
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Nenhum destino: o caminho de volta para casa e o ocioso
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_no_destination")
    @TestHolder(description = "Records the home path and the idle walk of stacks in a network with no destination.")
    public static void noDestination(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(5, 1, 3);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 3, 0, 0, basic()).t(builder, 3, 0, 1, basic(), null).t(builder, 3, 0, 2, basic(), null)
                  .t(builder, 2, 0, 1, basic(), null);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("no_destination", helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 0};
                rec.query("no destination at all", new int[]{1, 0, 0}, null, STONE, 10, 0, home, false, null);
                rec.idle("idle with a reachable home", new int[]{2, 0, 0}, null, STONE, 10, home, null, 0, new int[]{1, 0, 0}, null);
                rec.idle("idle with a home that is not a destination", new int[]{2, 0, 0}, null, STONE, 10, new int[]{0, 0, 1}, null, 0, new int[]{1, 0, 0}, null);
                //The walk: each step starts where the last path ended, with the stack the last step left
                int[] at = {2, 0, 0};
                TransporterStack stack = null;
                for (int i = 0; i < 8; i++) {
                    rec.idle("walk " + i, at, null, STONE, 10, null, null, 50, new int[]{1, 0, 0}, stack);
                    stack = rec.lastIdle;
                    if (!rec.lastFound || !stack.hasPath()) {
                        break;
                    }
                    BlockPos end = BlockPos.of(stack.getPath().getLong(0)).subtract(rec.origin);
                    int[] endAt = {end.getX(), end.getY(), end.getZ()};
                    boolean isTransporter = false;
                    for (int[] p : layout.transporters) {
                        isTransporter |= p[0] == endAt[0] && p[1] == endAt[1] && p[2] == endAt[2];
                    }
                    if (isTransporter) {
                        at = endAt;
                    }
                    stack.progress = 50;
                }
                rec.idle("idle, dir kept east", new int[]{1, 0, 0}, null, STONE, 10, null, Direction.EAST, 0, new int[]{0, 0, 0}, null);
                rec.idle("idle, dir kept west (blocked by the chest)", new int[]{3, 0, 0}, null, STONE, 10, null, Direction.WEST, 0, new int[]{0, 0, 0}, null);
                rec.idle("idle, dir kept up (nothing there)", new int[]{3, 0, 0}, null, STONE, 10, null, Direction.UP, 0, new int[]{0, 0, 0}, null);
            });
        }).thenSucceed());
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_single")
    @TestHolder(description = "Records the idle path of a single transporter between two chests.")
    public static void single(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(5, 1, 1);
            layout.chest(builder, 0, 0, 0).t(builder, 1, 0, 0, basic(), null).chest(builder, 2, 0, 0).t(builder, 4, 0, 0, basic(), null);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec("single", helper, layout);
            run(rec, () -> {
                rec.query("only chests, the first is the home", new int[]{1, 0, 0}, null, STONE, 10, 0, new int[]{0, 0, 0}, false, null);
                rec.idle("idle, no home, came from the west", new int[]{1, 0, 0}, null, STONE, 10, null, null, 0, new int[]{0, 0, 0}, null);
                rec.idle("idle, no home, came from the east", new int[]{1, 0, 0}, null, STONE, 10, null, null, 0, new int[]{2, 0, 0}, null);
                rec.idle("idle, no home, progress 50, came from the west", new int[]{1, 0, 0}, null, STONE, 10, null, null, 50, new int[]{0, 0, 0}, null);
                rec.idle("idle, east dir", new int[]{1, 0, 0}, null, STONE, 10, null, Direction.EAST, 0, new int[]{0, 0, 0}, null);
                rec.idle("a transporter next to nothing", new int[]{4, 0, 0}, null, STONE, 10, null, null, 0, new int[]{3, 0, 0}, null);
            });
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Um laço na rede
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static void loop(final DynamicTest test, String name, boolean ultimateHalf) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(5, 1, 5);
            layout.chest(builder, 0, 0, 2).chest(builder, 4, 0, 2);
            //The ring around (2, 2), north half and south half
            layout.t(builder, 1, 0, 2, basic(), null).t(builder, 3, 0, 2, basic(), null);
            layout.t(builder, 1, 0, 1, basic(), null).t(builder, 2, 0, 1, basic(), null).t(builder, 3, 0, 1, basic(), null);
            BlockState south = ultimateHalf ? ultimate() : basic();
            layout.t(builder, 1, 0, 3, south, null).t(builder, 2, 0, 3, south, null).t(builder, 3, 0, 3, south, null);
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            Rec rec = new Rec(name, helper, layout);
            run(rec, () -> {
                int[] home = {0, 0, 2};
                rec.query("across the ring", new int[]{1, 0, 2}, null, STONE, 10, 0, home, false, null);
                rec.query("from the north west corner", new int[]{1, 0, 1}, null, STONE, 10, 0, home, false, null);
                rec.query("from the south west corner", new int[]{1, 0, 3}, null, STONE, 10, 0, home, false, null);
                rec.query("from the top middle", new int[]{2, 0, 1}, null, STONE, 10, 0, home, false, null);
                rec.query("from the east side, the west chest is the destination", new int[]{3, 0, 2}, null, STONE, 10, 0, new int[]{4, 0, 2}, false, null);
                rec.idle("idle on the ring", new int[]{2, 0, 1}, null, STONE, 10, null, null, 0, new int[]{1, 0, 1}, null);
            });
        }).thenSucceed());
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_loop")
    @TestHolder(description = "Records the path across a ring of transporters where both halves cost the same.")
    public static void loopTie(final DynamicTest test) {
        loop(test, "loop_tie", false);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_transporter_loop_ultimate")
    @TestHolder(description = "Records the path across a ring of transporters where one half is cheaper.")
    public static void loopUltimate(final DynamicTest test) {
        loop(test, "loop_ultimate", true);
    }
}
