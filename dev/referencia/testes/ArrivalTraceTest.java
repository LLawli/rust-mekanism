package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.AutomationType;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.recipes.cache.CachedRecipe.OperationTracker.RecipeError;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava, tick a tick, o Metallurgic Infuser recebendo um item de entrada e um item que vira infusão, com o tanque vazio, em três
/// ordens: a entrada primeiro, a conversão primeiro, e os dois no mesmo tick. Cada item entra como um jogador o põe (`insert` com
/// `AutomationType.MANUAL`, o que o menu usa), e a marca guarda quanto o slot aceitou. Um par por receita de infusão portada e por
/// item que converte para o químico dela. A máquina é o bloco do gabarito, sem jogador (a capacidade de energia é a de uma máquina recém-posta, 20). A energia fica cheia a cada tick, e entre uma fase e outra a máquina é esvaziada e fica
/// parada alguns ticks.
@ForEachTest(groups = "trace")
public class ArrivalTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 0, 0);
    private static final int TIMEOUT = 240 * SharedConstants.TICKS_PER_SECOND;
    private static final int INFUSION = 0;
    private static final int INPUT = 1;
    private static final int OUTPUT = 2;
    private static final int ENERGY = 3;
    /// Ticks entre a primeira chegada e a segunda, e depois da última, e a pausa entre fases.
    private static final int GAP = 6;
    private static final int AFTER = 14;
    private static final int PAUSE = 4;

    /// O item de entrada da receita, o item que vira o químico dela, e o nome do par.
    private record Pair(String label, String input, String conversion) {
    }

    private static final List<Pair> PAIRS = List.of(
          new Pair("bio_dirt", "dirt", "mekanism:bio_fuel"),
          new Pair("bio_sand", "sand", "mekanism:bio_fuel"),
          new Pair("bio_clay", "clay", "mekanism:bio_fuel"),
          new Pair("fungi_dirt", "dirt", "red_mushroom"),
          new Pair("fungi_nylium", "crimson_nylium", "crimson_fungus"),
          new Pair("redstone_dust", "copper_ingot", "redstone"),
          new Pair("redstone_block", "copper_ingot", "redstone_block"),
          new Pair("diamond_dust", "mekanism:alloy_infused", "mekanism:dust_diamond"),
          new Pair("diamond_enriched", "mekanism:alloy_infused", "mekanism:enriched_diamond"),
          new Pair("gold_dust", "blackstone", "mekanism:dust_gold"),
          new Pair("gold_enriched", "blackstone", "mekanism:enriched_gold"),
          new Pair("obsidian_dust", "mekanism:alloy_reinforced", "mekanism:dust_refined_obsidian"),
          new Pair("obsidian_enriched", "mekanism:alloy_reinforced", "mekanism:enriched_refined_obsidian"));

    private static final List<RecipeError> ERRORS = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_INPUT,
          RecipeError.NOT_ENOUGH_OUTPUT_SPACE, RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT, RecipeError.NOT_ENOUGH_SECONDARY_INPUT);
    private static final List<String> ERROR_NAMES = List.of("energy", "input", "output_space", "wrong_output", "secondary");

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_arrival")
    @TestHolder(description = "Records the Metallurgic Infuser with the recipe input and the item that converts into the infusion arriving in each order, with an empty tank.")
    public static void infuserArrival(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.METALLURGIC_INFUSER.defaultState()));
        Scene scene = new Scene();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.place(helper));
            sequence.thenExecuteAfter(2, () -> {
            });
            for (Pair pair : PAIRS) {
                // A entrada primeiro, depois a conversão.
                sequence.thenExecuteAfter(PAUSE, () -> scene.reset());
                sequence.thenExecuteAfter(1, () -> scene.arrive(pair, "input"));
                sequence.thenExecuteAfter(GAP, () -> scene.arrive(pair, "conversion"));
                sequence.thenExecuteAfter(AFTER, () -> {
                });
                // A conversão primeiro, depois a entrada.
                sequence.thenExecuteAfter(PAUSE, () -> scene.reset());
                sequence.thenExecuteAfter(1, () -> scene.arrive(pair, "conversion"));
                sequence.thenExecuteAfter(GAP, () -> scene.arrive(pair, "input"));
                sequence.thenExecuteAfter(AFTER, () -> {
                });
                // Os dois no mesmo tick, a entrada primeiro.
                sequence.thenExecuteAfter(PAUSE, () -> scene.reset());
                sequence.thenExecuteAfter(1, () -> {
                    scene.arrive(pair, "input");
                    scene.arrive(pair, "conversion");
                });
                sequence.thenExecuteAfter(GAP + AFTER, () -> {
                });
            }
            sequence.thenExecuteAfter(PAUSE, () -> scene.finish());
            sequence.thenSucceed();
        });
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    private static final class Scene {
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;

        void place(MekGameTestHelper helper) {
            this.helper = helper;
        }

        private TileEntityMetallurgicInfuser tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
            return (TileEntityMetallurgicInfuser) tile;
        }

        private IInventorySlot slot(int index) {
            return tile().getInventorySlots().get(index);
        }

        private MachineEnergyContainer<?> energy() {
            return (MachineEnergyContainer<?>) tile().getEnergyContainer();
        }

        /// Esvazia a máquina: os slots, o tanque, e a energia cheia.
        void reset() {
            marks.add("reset");
            for (int i = 0; i < tile().getInventorySlots().size(); i++) {
                slot(i).setContents(ItemResource.EMPTY, 0, null);
            }
            tile().infusionTank.setContents(mekanism.api.chemical.ChemicalResource.EMPTY, 0, null);
            energy().setEnergy(energy().getCapacityAsLong(), null);
        }

        /// Um jogador põe o item no slot, como o menu faz: `MANUAL`.
        void arrive(Pair pair, String which) {
            boolean input = which.equals("input");
            String id = input ? pair.input : pair.conversion;
            int index = input ? INPUT : INFUSION;
            ItemResource resource = ItemResource.of(item(id));
            int moved;
            try (Transaction transaction = Transaction.openRoot()) {
                moved = slot(index).insert(resource, 2, transaction, AutomationType.MANUAL);
                transaction.commit();
            }
            marks.add("arrive " + pair.label + " " + which + " " + id.replace("mekanism:", "") + " 2 accepted " + moved);
        }

        private static void slot(StringBuilder line, IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                  .append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null || tile() == null) {
                return;
            }
            TileEntityMetallurgicInfuser infuser = tile();
            MachineEnergyContainer<?> energy = energy();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"progress\":").append(infuser.getRecipeProgress())
                  .append(",\"required\":").append(infuser.getTicksRequired())
                  .append(",\"energy\":").append(energy.getAmountAsLong())
                  .append(",\"capacity\":").append(energy.getCapacityAsLong())
                  .append(",\"per_tick\":").append(energy.getEnergyPerTick())
                  .append(",\"active\":").append(infuser.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(infuser.getBlockState()))
                  .append(",\"powered\":").append(infuser.isPowered())
                  .append(",\"comparator\":").append(infuser.getCurrentRedstoneLevel())
                  .append(",\"input\":[");
            slot(line, infuser.getInventorySlots().get(INPUT));
            line.append("],\"output\":[");
            slot(line, infuser.getInventorySlots().get(OUTPUT));
            line.append("],\"errors\":[");
            boolean first = true;
            for (int i = 0; i < ERRORS.size(); i++) {
                if (infuser.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(ERROR_NAMES.get(i)).append('"');
                    first = false;
                }
            }
            line.append(']');
            line.append(",\"tank\":").append(infuser.infusionTank.amountAsLong());
            String chemical = infuser.infusionTank.resource().isEmpty() ? "" : infuser.infusionTank.resource().typeHolder().getRegisteredName().replace("mekanism:", "");
            line.append(",\"tank_id\":\"").append(chemical).append('"');
            line.append(",\"infusion\":[");
            slot(line, infuser.getInventorySlots().get(INFUSION));
            line.append(']');
            lines.add(line.append('}').toString());
            // A energia fica cheia: o que se mede é a chegada dos itens, não a falta de energia.
            energy.setEnergy(energy.getCapacityAsLong(), null);
        }

        void finish() {
            helper = null;
            try {
                Path file = Path.of("trace", "infuser_arrival.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
