package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.AutomationType;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que um hopper faz com cada face das quatro máquinas portadas, o que o slot de infusão e o de energia aceitam de quem está
/// fora, e qual receita do Crusher o `findFirstRecipe` escolhe para `minecraft:melon_slice`. A máquina é posta como um jogador a poria
/// (com o item, que traz a configuração de lados padrão), em (1, 1, 1), com um hopper em cada vizinho, um por vez.
@ForEachTest(groups = "trace")
public class SlotsTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 1, 1);
    private static final int WAIT = 45;
    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final List<Direction> TAKE_FACES = List.of(Direction.UP, Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST);

    /// Os índices de `getInventorySlots()` de cada máquina: -1 onde ela não tem o slot.
    private record Spec(String name, Supplier<ItemLike> item, String valid, String product, int infusion, int input, int output, int energy) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(), "clay", "clay_ball", -1, 0, 1, 2);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), "cobblestone", "gravel", -1, 0, 1, 2);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(), "sand", "glass", -1, 0, 1, 2);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(), "dirt", "podzol", 0, 1, 2, 3);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slots")
    @TestHolder(description = "Records hoppers around the Enrichment Chamber and the rules of its energy slot.")
    public static void chamberHoppers(final DynamicTest test) {
        hoppers(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slots")
    @TestHolder(description = "Records hoppers around the Crusher and the rules of its energy slot.")
    public static void crusherHoppers(final DynamicTest test) {
        hoppers(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slots")
    @TestHolder(description = "Records hoppers around the Energized Smelter and the rules of its energy slot.")
    public static void smelterHoppers(final DynamicTest test) {
        hoppers(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slots")
    @TestHolder(description = "Records hoppers around the Metallurgic Infuser and the rules of its infusion and energy slots.")
    public static void infuserHoppers(final DynamicTest test) {
        hoppers(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_slots")
    @TestHolder(description = "Records which crushing recipes match a melon slice, in the order of the loaded list, and which one findFirstRecipe picks.")
    public static void crusherMelon(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<String> lines = new ArrayList<>();
                  ItemStack melon = new ItemStack(Items.MELON_SLICE);
                  var holders = MekanismRecipeType.CRUSHING.getRecipes(helper.getLevel());
                  var found = MekanismRecipeType.CRUSHING.getInputCache().findFirstRecipe(helper.getLevel(), melon);
                  StringBuilder matches = new StringBuilder();
                  int position = 0;
                  for (RecipeHolder<?> holder : holders) {
                      if (matchesMelon(holder, melon)) {
                          matches.append(matches.length() == 0 ? "" : ",").append("{\"index\":").append(position).append(",\"id\":\"")
                                .append(holder.id().identifier().getPath()).append("\"}");
                      }
                      position++;
                  }
                  StringBuilder order = new StringBuilder();
                  for (RecipeHolder<?> holder : holders) {
                      order.append(order.length() == 0 ? "" : ",").append('"').append(holder.id().identifier().getPath()).append('"');
                  }
                  lines.add("{\"recipes\":" + holders.size() + ",\"order\":[" + order + "],\"matching\":[" + matches + "],\"first\":" + (found == null ? "null"
                        : "\"" + found.id().identifier().getPath() + "\"") + "}");
                  write("crusher_melon", lines);
              })
              .thenSucceed()
        );
    }

    private static boolean matchesMelon(RecipeHolder<?> holder, ItemStack melon) {
        return holder.value() instanceof mekanism.api.recipes.ItemStackToItemStackRecipe<?, ?> recipe && recipe.getInput().test(melon);
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

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    private static void hoppers(DynamicTest test, Spec spec) {
        template(test);
        Scene scene = new Scene(spec);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.place(helper));
            sequence.thenExecuteAfter(2, () -> scene.probeSlots());
            // O hopper de baixo, com cada estado em que a máquina guarda um item que ele poderia tirar.
            sequence.thenExecuteAfter(1, () -> scene.give("parked", null, 0, null));
            sequence.thenExecuteAfter(WAIT, scene::record);
            if (spec.infusion >= 0) {
                sequence.thenExecuteAfter(1, () -> scene.give("stick_in_infusion", "stick", 3, null));
                sequence.thenExecuteAfter(WAIT, scene::record);
                sequence.thenExecuteAfter(1, () -> scene.give("bio_fuel_tank_full_bio", "mekanism:bio_fuel", 3, "bio:1000"));
                sequence.thenExecuteAfter(WAIT, scene::record);
                sequence.thenExecuteAfter(1, () -> scene.give("bio_fuel_tank_fungi", "mekanism:bio_fuel", 3, "fungi:500"));
                sequence.thenExecuteAfter(WAIT, scene::record);
                sequence.thenExecuteAfter(1, () -> scene.give("bio_fuel_tank_empty", "mekanism:bio_fuel", 3, "empty:0"));
                sequence.thenExecuteAfter(WAIT, scene::record);
            }
            for (Direction face : TAKE_FACES) {
                sequence.thenExecuteAfter(1, () -> scene.take(face));
                sequence.thenExecuteAfter(WAIT, scene::record);
            }
            sequence.thenExecuteAfter(1, () -> scene.finish());
            sequence.thenSucceed();
        });
    }

    /// Uma máquina e os seis vizinhos que podem ter um hopper.
    private static final class Scene {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private String phase = "";
        private String front = "";

        Scene(Spec spec) {
            this.spec = spec;
        }

        void place(MekGameTestHelper helper) {
            this.helper = helper;
            Player player = helper.makeMockPlayerLookingAt(1, 0, 1, Direction.UP);
            helper.useOn(new BlockPos(1, 0, 1), new ItemStack(spec.item.get()), player, Direction.UP);
            front = tile().getDirection().getSerializedName();
            lines.add("{\"machine\":\"" + spec.name + "\",\"front\":\"" + front + "\"}");
        }

        TileEntityMekanism tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
            return (TileEntityMekanism) tile;
        }

        private IInventorySlot slot(int index) {
            return tile().getInventorySlots().get(index);
        }

        private void setSlot(int index, String id, int count) {
            if (index < 0) {
                return;
            }
            slot(index).setContents(id == null || count == 0 ? ItemResource.EMPTY : ItemResource.of(item(id)), id == null ? 0 : count, null);
        }

        private void setTank(String spec) {
            if (this.spec.infusion < 0 || spec == null) {
                return;
            }
            var tank = ((TileEntityMetallurgicInfuser) tile()).infusionTank;
            String[] parts = spec.split(":");
            if (parts[0].equals("empty")) {
                tank.setContents(ChemicalResource.EMPTY, 0, null);
                return;
            }
            Holder<Chemical> chemical = helper.getLevel().registryAccess().getOrThrow(parts[0].equals("bio") ? ChemicalIds.BIO : ChemicalIds.FUNGI);
            tank.setContents(ChemicalResource.of(chemical), Long.parseLong(parts[1]), null);
        }

        private void clearAll() {
            for (int i = 0; i < tile().getInventorySlots().size(); i++) {
                setSlot(i, null, 0);
            }
            setTank("empty:0");
            for (Direction face : Direction.values()) {
                BlockPos pos = around(face);
                if (helper.getLevel().getBlockState(helper.absolutePos(pos)).is(Blocks.HOPPER)) {
                    ((Container) helper.getLevel().getBlockEntity(helper.absolutePos(pos))).clearContent();
                }
                helper.setBlock(pos, Blocks.AIR.defaultBlockState());
            }
        }

        private static BlockPos around(Direction face) {
            return MACHINE.relative(face);
        }

        private BlockState hopper(Direction pointing) {
            return Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING, pointing);
        }

        /// O hopper de baixo, e uma máquina com os slots como `infusionItem` e `tank` dizem. A entrada e a saída levam a receita.
        void give(String label, String infusionItem, int infusionCount, String tank) {
            clearAll();
            phase = "give_" + label;
            setSlot(spec.input, spec.valid, 5);
            setSlot(spec.output, spec.product, 5);
            setTank(tank);
            setSlot(spec.infusion, infusionItem, infusionCount);
            // Um item que ninguém converte, parado no slot de energia: um hopper de baixo não chega a ele, as costas é que chegam.
            setSlot(spec.energy, "stick", 2);
            helper.setBlock(around(Direction.DOWN), hopper(Direction.DOWN));
        }

        /// Um hopper em `face`, apontando para a máquina, com itens de todos os tipos que alguma regra da máquina conhece.
        void take(Direction face) {
            clearAll();
            phase = "take_" + face.getSerializedName();
            Direction pointing = face == Direction.UP ? Direction.DOWN : face.getOpposite();
            BlockPos pos = around(face);
            helper.setBlock(pos, hopper(pointing));
            Container container = (Container) helper.getLevel().getBlockEntity(helper.absolutePos(pos));
            String[] items = {"stick", spec.valid, "redstone", "mekanism:bio_fuel", "coal"};
            for (int i = 0; i < items.length; i++) {
                container.setItem(i, new ItemStack(item(items[i]), 1));
            }
        }

        private void stacks(StringBuilder line, Container container) {
            boolean first = true;
            for (int i = 0; i < container.getContainerSize(); i++) {
                ItemStack stack = container.getItem(i);
                if (!stack.isEmpty()) {
                    line.append(first ? "" : ",").append("[\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",")
                          .append(stack.getCount()).append(']');
                    first = false;
                }
            }
        }

        void record() {
            StringBuilder line = new StringBuilder("{\"machine\":\"").append(spec.name).append("\",\"phase\":\"").append(phase).append("\",\"hoppers\":{");
            boolean first = true;
            for (Direction face : Direction.values()) {
                BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(around(face)));
                if (entity instanceof Container container && helper.getLevel().getBlockState(helper.absolutePos(around(face))).is(Blocks.HOPPER)) {
                    line.append(first ? "" : ",").append('"').append(face.getSerializedName()).append("\":[");
                    stacks(line, container);
                    line.append(']');
                    first = false;
                }
            }
            line.append("},\"slots\":[");
            List<IInventorySlot> slots = tile().getInventorySlots();
            for (int i = 0; i < slots.size(); i++) {
                ItemStack stack = slots.get(i).asItemStack();
                line.append(i == 0 ? "" : ",").append("[\"").append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                      .append("\",").append(stack.getCount()).append(']');
            }
            line.append(']');
            if (spec.infusion >= 0) {
                var tank = ((TileEntityMetallurgicInfuser) tile()).infusionTank;
                String id = tank.resource().isEmpty() ? "" : tank.resource().typeHolder().getRegisteredName().replace("mekanism:", "");
                line.append(",\"tank\":[\"").append(id).append("\",").append(tank.amountAsLong()).append(']');
            }
            lines.add(line.append('}').toString());
        }

        /// O que a máquina deixa um item de fora fazer com o slot de energia, e com o de infusão. `insert` e `extract` simulados, com cada
        /// tipo de automação, e para o de infusão cada estado do tanque e da entrada.
        void probeSlots() {
            clearAll();
            List<String> items = List.of("stick", "redstone", "redstone_block", "coal", "diamond", "mekanism:bio_fuel", "red_mushroom");
            for (String id : items) {
                probe("energy", spec.energy, id, "-", "-");
            }
            if (spec.infusion >= 0) {
                for (String tank : List.of("empty:0", "bio:500", "bio:1000", "fungi:500")) {
                    for (String input : List.of("", "dirt")) {
                        clearAll();
                        setTank(tank);
                        setSlot(spec.input, input.isEmpty() ? null : input, 1);
                        for (String id : items) {
                            probe("infusion", spec.infusion, id, tank, input);
                        }
                    }
                }
            }
            clearAll();
        }

        private void probe(String which, int index, String id, String tank, String input) {
            ItemResource resource = ItemResource.of(item(id));
            StringBuilder line = new StringBuilder("{\"machine\":\"").append(spec.name).append("\",\"probe\":\"").append(which).append("\",\"tank\":\"")
                  .append(tank).append("\",\"input\":\"").append(input).append("\",\"item\":\"").append(id).append("\"");
            IInventorySlot slot = slot(index);
            for (String op : List.of("insert", "extract")) {
                line.append(",\"").append(op).append("\":{");
                boolean first = true;
                for (AutomationType automation : AutomationType.values()) {
                    slot.setContents(op.equals("insert") ? ItemResource.EMPTY : resource, op.equals("insert") ? 0 : 3, null);
                    int moved;
                    try (Transaction simulation = Transaction.openRoot()) {
                        moved = op.equals("insert") ? slot.insert(resource, 1, simulation, automation) : slot.extract(resource, 3, simulation, automation);
                    }
                    line.append(first ? "" : ",").append('"').append(automation.name()).append("\":").append(moved);
                    first = false;
                }
                line.append('}');
            }
            slot.setContents(ItemResource.EMPTY, 0, null);
            lines.add(line.append('}').toString());
        }

        void finish() {
            write("slots_" + spec.name, lines);
        }
    }
}
