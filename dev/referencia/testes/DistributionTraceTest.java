package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.content.network.distribution.EnergyHandlerTarget;
import mekanism.common.content.network.distribution.EnergySaveTarget;
import mekanism.common.content.network.distribution.ResourceHandlerTarget;
import mekanism.common.util.EmitUtils;
import mekanism.common.util.EnergyUtils;
import net.minecraft.SharedConstants;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.energy.SimpleEnergyHandler;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.fluid.FluidStacksResourceHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.neoforge.transfer.transaction.TransactionContext;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import mekanism.common.tests.helpers.MekGameTestHelper;

/// Frente `rede-distribuicao`: grava a distribuicao generica (`SplitInfo`, `Target`, `EmitUtils.sendToAcceptors`, `EnergyUtils.emit` de lista e os alvos de energia, de
/// recurso e de save) com handlers de mentira, sem mundo. Um `runGameTestServer`, o arquivo `distribution.jsonl`: cada linha e um caso (`kind`, os handlers com capacidade,
/// limite de insercao e energia inicial, a ordem em que entram no alvo, com repeticao, e a quantidade a dividir) e o que o original fez (o total enviado, o estado final de
/// cada handler e, nos handlers de energia, cada chamada de `insert`, na ordem: handler, pedido, aceito).
@ForEachTest(groups = "trace")
public class DistributionTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-12";
    private static final long SEED = 20261112L;
    private static final int RANDOM_CASES = 700;

    private static final long[] CAPS = {0, 1, 2, 5, 10, 33, 100, 1000, 100_000, Integer.MAX_VALUE};
    private static final long[] BIG_CAPS = {0, 1, 7, 100, 1000, 3_000_000_000L, 5_000_000_000L, Long.MAX_VALUE};
    private static final long[] AMOUNTS = {0, 1, 2, 3, 7, 10, 11, 99, 100, 101, 999, 1000, 4097, 100_000, Integer.MAX_VALUE, 3_000_000_000L};
    private static final long[] BIG_AMOUNTS = {1, 7, 100, 1000, 4_999_999_999L, 6_000_000_000L, Long.MAX_VALUE};

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_distribution")
    @TestHolder(description = "Records Target/SplitInfo/EmitUtils/EnergyUtils.emit with fake handlers: sent, final states and every insert call.")
    public static void distribution(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(DistributionTraceTest::all)
              .thenSucceed());
    }

    private record Spec(long cap, long maxInsert, long energy) {
    }

    private record Case(String kind, List<Spec> handlers, List<Integer> order, long amount) {
    }

    /// Um handler de energia que anota cada `insert` (handler, pedido, aceito), como o `EnergyHandler` que um vizinho expoe.
    private static final class LogEnergy extends SimpleEnergyHandler {

        private final int index;
        private final List<long[]> log;

        LogEnergy(int index, Spec spec, List<long[]> log) {
            super((int) spec.cap, (int) spec.maxInsert, 0, (int) spec.energy);
            this.index = index;
            this.log = log;
        }

        @Override
        public int insert(int amount, TransactionContext transaction) {
            int got = super.insert(amount, transaction);
            log.add(new long[]{index, amount, got});
            return got;
        }
    }

    private static void all() {
        List<Case> cases = new ArrayList<>();
        // Casos escolhidos a mao: o resto, o vizinho que quer menos, o que quer mais, o que repete.
        add(cases, "target", specs(100, 100, 0, 100, 100, 0, 100, 100, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 2), 101);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 2), 102);
        add(cases, "target", specs(5, 5, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(5, 5, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 2), 101);
        add(cases, "target", specs(1000, 1000, 0, 5, 5, 0, 1000, 1000, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 5, 5, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 1, 0), 100);
        add(cases, "target", specs(1000, 1000, 0, 1000, 1000, 0, 1000, 1000, 0), List.of(0, 0, 0), 100);
        add(cases, "target", specs(30, 30, 0, 1000, 1000, 0), List.of(0, 1, 0), 100);
        add(cases, "target", specs(30, 30, 0, 1000, 1000, 0), List.of(1, 0, 1, 0), 101);
        add(cases, "target", specs(0, 0, 0, 0, 0, 0), List.of(0, 1), 100);
        add(cases, "target", specs(40, 40, 0, 60, 60, 0, 70, 70, 0), List.of(0, 1, 2), 100);
        add(cases, "target", specs(40, 40, 0, 60, 60, 0, 70, 70, 0), List.of(2, 1, 0), 100);
        add(cases, "target", specs(10, 10, 0, 10, 10, 0, 10, 10, 0, 1000, 1000, 0), List.of(0, 1, 2, 3), 100);
        add(cases, "target", specs(1000, 1000, 0), List.of(0), 100);
        add(cases, "target", specs(1000, 1000, 0), List.of(), 100);
        add(cases, "target_int", specs(1000, 1000, 0, 1000, 1000, 0), List.of(0, 1), 101);
        add(cases, "target_int", specs(1000, 1000, 0, 1000, 1000, 0), List.of(0, 1), 0);
        add(cases, "emit_list", specs(1000, 1000, 0, 1000, 1000, 0), List.of(0, 1), 101);
        add(cases, "emit_list", specs(1000, 1000, 0), List.of(0), 101);
        add(cases, "emit_list", specs(1000, 1000, 0, 1000, 1000, 0), List.of(0, 1), 0);
        add(cases, "emit_list", specs(1000, 1000, 0, 1000, 1000, 0), List.of(), 50);
        add(cases, "save", specs(100, 0, 40, 100, 0, 40, 100, 0, 40), List.of(0, 1, 2), 150);
        add(cases, "save", specs(10, 0, 0, 100, 0, 0, 100, 0, 0), List.of(0, 1, 2), 100);
        add(cases, "save", specs(100, 0, 0, 100, 0, 0), List.of(0, 1, 0), 120);
        add(cases, "fluid", specs(100, 0, 0, 100, 0, 0, 100, 0, 0), List.of(0, 1, 2), 150);
        add(cases, "fluid", specs(10, 0, 0, 100, 0, 0, 100, 0, 0), List.of(0, 1, 2), 101);

        Random random = new Random(SEED);
        String[] kinds = {"target", "target", "target", "target_int", "emit_list", "save", "save", "fluid"};
        for (int i = 0; i < RANDOM_CASES; i++) {
            String kind = kinds[random.nextInt(kinds.length)];
            boolean big = kind.equals("save") && random.nextInt(3) == 0;
            int unique = 1 + random.nextInt(6);
            List<Spec> handlers = new ArrayList<>();
            for (int h = 0; h < unique; h++) {
                long cap = big ? BIG_CAPS[random.nextInt(BIG_CAPS.length)] : CAPS[random.nextInt(CAPS.length)];
                long maxInsert = kind.equals("save") || kind.equals("fluid") ? 0 : (random.nextInt(3) == 0 ? CAPS[random.nextInt(CAPS.length)] : cap);
                long energy = random.nextInt(3) == 0 && cap > 0 && !kind.equals("save") ? (long) (random.nextDouble() * cap) : 0;
                if (kind.equals("save")) {
                    energy = random.nextInt(2) == 0 ? 0 : (long) (random.nextDouble() * cap);
                }
                handlers.add(new Spec(cap, maxInsert, energy));
            }
            int length = 1 + random.nextInt(6);
            List<Integer> order = new ArrayList<>();
            boolean repeat = random.nextInt(4) == 0;
            if (repeat) {
                for (int o = 0; o < length; o++) {
                    order.add(random.nextInt(unique));
                }
            } else {
                for (int o = 0; o < unique; o++) {
                    order.add(o);
                }
                java.util.Collections.shuffle(order, random);
            }
            long amount;
            if (big) {
                amount = BIG_AMOUNTS[random.nextInt(BIG_AMOUNTS.length)];
            } else if (random.nextInt(3) == 0) {
                amount = AMOUNTS[random.nextInt(AMOUNTS.length)];
            } else {
                amount = random.nextInt(2000);
            }
            if (kind.equals("target_int") || kind.equals("fluid")) {
                amount = Math.min(amount, Integer.MAX_VALUE);
            }
            cases.add(new Case(kind, handlers, order, amount));
        }

        List<String> lines = new ArrayList<>();
        JsonObject meta = new JsonObject();
        meta.addProperty("commit", COMMIT);
        meta.addProperty("date", DATE);
        meta.addProperty("file", "distribution.jsonl");
        meta.addProperty("description", "Target/SplitInfo/EmitUtils.sendToAcceptors/EnergyUtils.emit(List) with fake handlers, no world. kinds: target (EnergyHandlerTarget with the Collection constructor, sendToAcceptors with a long), target_int (the int overload, saturated result), emit_list (EnergyUtils.emit(List<EnergyHandler>, long)), save (EnergySaveTarget over BasicEnergyContainer, addHandler, inside a root transaction that commits; startSaveHandling zeroes the container), fluid (ResourceHandlerTarget over a one-slot FluidStacksResourceHandler of water).");
        meta.addProperty("fields", "kind; handlers (cap, max_insert, energy: the initial state, applied to target, target_int, emit_list and save; the fluid tanks always start empty, whatever energy says; max_insert is 0 for save and fluid); order (indexes into handlers, in the order they enter the target, a repeated index is the same object); amount; sent; finals (energy or fluid amount of each handler); calls ([handler, requested, accepted] of every insert, only for target, target_int and emit_list).");
        JsonObject head = new JsonObject();
        head.add("meta", meta);
        lines.add(head.toString());
        for (Case c : cases) {
            lines.add(run(c).toString());
        }
        try {
            Path file = Path.of("trace", "distribution.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static List<Spec> specs(long... values) {
        List<Spec> out = new ArrayList<>();
        for (int i = 0; i < values.length; i += 3) {
            out.add(new Spec(values[i], values[i + 1], values[i + 2]));
        }
        return out;
    }

    private static void add(List<Case> cases, String kind, List<Spec> handlers, List<Integer> order, long amount) {
        cases.add(new Case(kind, handlers, order, amount));
    }

    private static JsonObject run(Case c) {
        JsonObject line = new JsonObject();
        line.addProperty("kind", c.kind);
        JsonArray handlers = new JsonArray();
        for (Spec spec : c.handlers) {
            JsonObject h = new JsonObject();
            h.addProperty("cap", spec.cap);
            h.addProperty("max_insert", spec.maxInsert);
            h.addProperty("energy", spec.energy);
            handlers.add(h);
        }
        line.add("handlers", handlers);
        JsonArray order = new JsonArray();
        c.order.forEach(order::add);
        line.add("order", order);
        line.addProperty("amount", c.amount);
        long sent;
        long[] finals = new long[c.handlers.size()];
        List<long[]> log = new ArrayList<>();
        switch (c.kind) {
            case "target", "target_int", "emit_list" -> {
                List<LogEnergy> objects = new ArrayList<>();
                for (int i = 0; i < c.handlers.size(); i++) {
                    objects.add(new LogEnergy(i, c.handlers.get(i), log));
                }
                List<EnergyHandler> list = new ArrayList<>();
                for (int index : c.order) {
                    list.add(objects.get(index));
                }
                if (c.kind.equals("target")) {
                    sent = EmitUtils.sendToAcceptors(new EnergyHandlerTarget(list), c.amount, null, null);
                } else if (c.kind.equals("target_int")) {
                    sent = EmitUtils.sendToAcceptors(new EnergyHandlerTarget(list), (int) c.amount, null, null);
                } else {
                    sent = EnergyUtils.emit(list, c.amount, null);
                }
                for (int i = 0; i < finals.length; i++) {
                    finals[i] = objects.get(i).getAmountAsLong();
                }
            }
            case "save" -> {
                List<BasicEnergyContainer> containers = new ArrayList<>();
                for (Spec spec : c.handlers) {
                    BasicEnergyContainer container = BasicEnergyContainer.create(spec.cap, null);
                    container.setEnergy(spec.energy, null);
                    containers.add(container);
                }
                try (Transaction tx = Transaction.openRoot()) {
                    List<EnergySaveTarget.SaveHandler> saves = new ArrayList<>();
                    for (BasicEnergyContainer container : containers) {
                        saves.add(EnergySaveTarget.SaveHandler.startSaveHandling(container, tx));
                    }
                    EnergySaveTarget target = new EnergySaveTarget(c.order.size());
                    for (int index : c.order) {
                        target.addHandler(saves.get(index));
                    }
                    sent = EmitUtils.sendToAcceptors(target, c.amount, null, tx);
                    tx.commit();
                }
                for (int i = 0; i < finals.length; i++) {
                    finals[i] = containers.get(i).getAmountAsLong();
                }
            }
            case "fluid" -> {
                List<FluidStacksResourceHandler> tanks = new ArrayList<>();
                for (Spec spec : c.handlers) {
                    FluidStacksResourceHandler tank = new FluidStacksResourceHandler(1, (int) spec.cap);
                    tanks.add(tank);
                }
                List<ResourceHandler<FluidResource>> list = new ArrayList<>();
                for (int index : c.order) {
                    list.add(tanks.get(index));
                }
                sent = EmitUtils.sendToAcceptors(new ResourceHandlerTarget<>(list), c.amount, FluidResource.of(new FluidStack(Fluids.WATER, 1)), null);
                for (int i = 0; i < finals.length; i++) {
                    finals[i] = tanks.get(i).getAmountAsLong(0);
                }
            }
            default -> throw new IllegalStateException(c.kind);
        }
        line.addProperty("sent", sent);
        JsonArray finalArray = new JsonArray();
        for (long value : finals) {
            finalArray.add(value);
        }
        line.add("finals", finalArray);
        if (!c.kind.equals("save") && !c.kind.equals("fluid")) {
            JsonArray calls = new JsonArray();
            for (long[] call : log) {
                JsonArray entry = new JsonArray();
                entry.add(call[0]);
                entry.add(call[1]);
                entry.add(call[2]);
                calls.add(entry);
            }
            line.add("calls", calls);
        }
        return line;
    }
}
