package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import mekanism.api.text.EnumColor;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.recipe.upgrade.MekanismShapedRecipe;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.prefab.TileEntityConfigurableMachine;
import mekanism.common.util.EnergyUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.CraftingInput;
import net.minecraft.world.item.crafting.CraftingRecipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import mekanism.common.util.ItemAccessUtils;

/// Frente `mek-data`: grava as receitas `mekanism:mek_data` (`MekanismShapedRecipe`) e o que a `assemble` delas copia das entradas. Um `runGameTestServer`, os arquivos
/// `mek_data_*.jsonl`: `recipes` (as receitas de tipo `MekanismShapedRecipe` e o que cada uma dá) e `assemble` (as entradas, o resultado, a energia somada e os componentes).
@ForEachTest(groups = "trace")
public class MekDataTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-11";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_mek_data")
    @TestHolder(description = "Records every recipe of the MekanismShapedRecipe class: id, group, result, and the registration order among the crafting recipes.")
    public static void recipes(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  List<String> lines = new ArrayList<>();
                  lines.add(meta("mek_data_recipes", "One line per recipe of the class MekanismShapedRecipe, in the order of RecipeMap.byType(CRAFTING): id, serializer, result item and count, "
                        + "width, height. The last line has the totals.").toString());
                  int index = 0;
                  int total = 0;
                  for (RecipeHolder<CraftingRecipe> holder : level.recipeAccess().recipeMap().byType(RecipeType.CRAFTING)) {
                      index++;
                      if (holder.value() instanceof MekanismShapedRecipe recipe) {
                          total++;
                          JsonObject line = new JsonObject();
                          line.addProperty("id", holder.id().identifier().toString());
                          line.addProperty("order", index);
                          line.addProperty("serializer", BuiltInRegistries.RECIPE_SERIALIZER.getKey(recipe.getSerializer()).toString());
                          ItemStack result = recipe.assemble(CraftingInput.EMPTY);
                          line.addProperty("empty_input_assembles_to", BuiltInRegistries.ITEM.getKey(result.getItem()).toString());
                          lines.add(line.toString());
                      }
                  }
                  JsonObject totals = new JsonObject();
                  totals.addProperty("total", total);
                  totals.addProperty("crafting", index);
                  lines.add(totals.toString());
                  write("mek_data_recipes", lines);
              })
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_mek_data")
    @TestHolder(description = "Records MekanismShapedRecipe.assemble and matches for energy cubes, a machine to its factory, and plain inputs: the energy added, clamped, the components kept and dropped.")
    public static void assemble(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> assembleAll(helper))
              .thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static JsonObject meta(String name, String description) {
        JsonObject meta = new JsonObject();
        meta.addProperty("commit", COMMIT);
        meta.addProperty("date", DATE);
        meta.addProperty("file", name + ".jsonl");
        meta.addProperty("description", description);
        JsonObject line = new JsonObject();
        line.add("meta", meta);
        return line;
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

    private static CraftingRecipe recipe(ServerLevel level, String id) {
        for (RecipeHolder<CraftingRecipe> holder : level.recipeAccess().recipeMap().byType(RecipeType.CRAFTING)) {
            if (holder.id().identifier().toString().equals(id)) {
                return holder.value();
            }
        }
        throw new IllegalStateException("no recipe " + id);
    }

    /// Uma pilha com `energy` (se >= 0) no componente de energia do item, pelo mesmo caminho que a receita usa para ler e escrever.
    private static ItemStack withEnergy(Item item, long energy) {
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        if (energy >= 0) {
            EnergyHandler handler = ContainerType.ENERGY.getCapOrUnexposed(access);
            var container = EnergyUtils.getEnergyContainer(handler);
            container.setEnergy(energy, null);
        }
        return ItemAccessUtils.asStack(access);
    }

    private static long energy(ItemStack stack) {
        return stack.isEmpty() ? 0 : ContainerType.ENERGY.getOrZero(ItemResource.of(stack));
    }

    private static long capacity(Item item) {
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        return EnergyUtils.getEnergyContainer(ContainerType.ENERGY.getCapOrUnexposed(access)).getCapacityAsLong();
    }

    private static JsonObject stackJson(ServerLevel level, ItemStack stack) {
        JsonObject out = new JsonObject();
        if (stack.isEmpty()) {
            out.addProperty("empty", true);
            return out;
        }
        out.addProperty("empty", false);
        out.addProperty("item", BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        out.addProperty("count", stack.getCount());
        out.addProperty("energy", energy(stack));
        out.add("patch", patchKeys(stack));
        JsonElement full = ItemStack.CODEC.encodeStart(level.registryAccess().createSerializationContext(JsonOps.INSTANCE), stack).getOrThrow(IllegalStateException::new);
        out.add("encoded", full);
        return out;
    }

    private static JsonArray patchKeys(ItemStack stack) {
        List<String> keys = new ArrayList<>();
        DataComponentPatch.SplitResult split = stack.getComponentsPatch().split();
        for (DataComponentType<?> type : split.added().keySet()) {
            keys.add(String.valueOf(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type)));
        }
        for (DataComponentType<?> type : split.removed()) {
            keys.add("!" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type));
        }
        Collections.sort(keys);
        JsonArray array = new JsonArray();
        keys.forEach(array::add);
        return array;
    }

    /// Uma linha: a receita, a grade 3x3 (de cima para baixo, `null` vazio), o `matches`, o `assemble` e a energia de cada entrada e da saída.
    private static void line(List<String> lines, ServerLevel level, String name, String id, List<ItemStack> grid, String note) {
        CraftingRecipe recipe = recipe(level, id);
        CraftingInput input = CraftingInput.of(3, 3, grid);
        JsonObject out = new JsonObject();
        out.addProperty("case", name);
        out.addProperty("recipe", id);
        out.addProperty("note", note);
        JsonArray inputs = new JsonArray();
        long sum = 0;
        for (ItemStack stack : grid) {
            inputs.add(stackJson(level, stack));
            sum += energy(stack);
        }
        out.add("inputs", inputs);
        out.addProperty("input_energy_sum", sum);
        out.addProperty("matches", recipe.matches(input, level));
        ItemStack result = recipe.assemble(input);
        out.add("result", stackJson(level, result));
        out.addProperty("result_capacity", result.isEmpty() ? 0 : capacity(result.getItem()));
        lines.add(out.toString());
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(net.minecraft.resources.Identifier.parse(id));
    }

    private static ItemStack of(Item item) {
        return new ItemStack(item);
    }

    private static void assembleAll(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        List<String> lines = new ArrayList<>();
        lines.add(meta("mek_data_assemble", "MekanismShapedRecipe.matches and assemble for named cases. inputs: the 9 grid cells (row by row) with count, energy (ContainerType.ENERGY.getOrZero), patch keys and the encoded stack; "
              + "input_energy_sum: the sum of the energies; matches / result: the recipe's answer; result_capacity: the energy capacity of the result item.").toString());
        Item alloy = item("mekanism:alloy_infused");
        Item osmium = item("mekanism:ingot_osmium");
        Item tablet = item("mekanism:energy_tablet");
        Item basicCube = MekanismBlocks.BASIC_ENERGY_CUBE.asItem();
        Item advancedCube = MekanismBlocks.ADVANCED_ENERGY_CUBE.asItem();
        long basicCap = capacity(basicCube);
        long advancedCap = capacity(advancedCube);
        long tabletCap = capacity(tablet);
        // advanced_energy_cube: AEA / IPI / AEA (A alloy infused, E energy tablet, I osmium ingot, P basic cube)
        List<ItemStack> plain = List.of(of(alloy), of(tablet), of(alloy), of(osmium), of(basicCube), of(osmium), of(alloy), of(tablet), of(alloy));
        line(lines, level, "cube_plain", "mekanism:energy_cube/advanced", plain, "no component on any input: assemble returns the template result");
        line(lines, level, "cube_energy_basic_full", "mekanism:energy_cube/advanced",
              List.of(of(alloy), of(tablet), of(alloy), of(osmium), withEnergy(basicCube, basicCap), of(osmium), of(alloy), of(tablet), of(alloy)),
              "the basic cube is full, the tablets empty: the result gets the cube's energy");
        line(lines, level, "cube_energy_sum", "mekanism:energy_cube/advanced",
              List.of(of(alloy), withEnergy(tablet, 1000), of(alloy), of(osmium), withEnergy(basicCube, 5000), of(osmium), of(alloy), withEnergy(tablet, 2500), of(alloy)),
              "three inputs with energy: the energies add up");
        line(lines, level, "cube_energy_clamped", "mekanism:energy_cube/advanced",
              List.of(of(alloy), withEnergy(tablet, tabletCap), of(alloy), of(osmium), withEnergy(basicCube, basicCap), of(osmium), of(alloy), withEnergy(tablet, tabletCap), of(alloy)),
              "full tablets and a full cube: the sum is clamped to the capacity of the result (" + advancedCap + ")");
        line(lines, level, "cube_energy_tablet_only", "mekanism:energy_cube/advanced",
              List.of(of(alloy), withEnergy(tablet, 777), of(alloy), of(osmium), of(basicCube), of(osmium), of(alloy), of(tablet), of(alloy)),
              "only a tablet has energy: it still goes to the result");
        // basic_energy_cube: pattern of energy_cube/basic
        // um pedaço de lixo no lugar certo não casa
        line(lines, level, "cube_wrong_item", "mekanism:energy_cube/advanced",
              List.of(of(alloy), of(tablet), of(alloy), of(osmium), of(Items.STONE), of(osmium), of(alloy), of(tablet), of(alloy)),
              "a stone in the middle: no match, assemble still builds the template result");
        // a máquina com configuração e energia, para a fábrica
        Player player = helper.makeMockPlayerLookingAt(P1.below(), Direction.UP);
        player.setYRot(Direction.SOUTH.toYRot());
        helper.useOn(P1.below(), new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()), player, Direction.UP);
        TileEntityMekanism tile = (TileEntityMekanism) level.getBlockEntity(helper.absolutePos(P1));
        if (tile instanceof TileEntityConfigurableMachine machine) {
            machine.getEjector().setStrictInput(true);
            machine.getEjector().setOutputColor(EnumColor.DARK_GREEN);
            machine.getEjector().setInputColor(mekanism.api.RelativeSide.LEFT, EnumColor.RED);
        } else {
            throw new IllegalStateException("not a configurable machine: " + tile);
        }
        ItemStack configured = new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem());
        configured.applyComponents(tile.collectComponents());
        ItemStack configuredWithEnergy = configured.copy();
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(configuredWithEnergy));
        var energyContainer = EnergyUtils.getEnergyContainer(ContainerType.ENERGY.getCapOrUnexposed(access));
        if (energyContainer != null) {
            energyContainer.setEnergy(1234, null);
            configuredWithEnergy = ItemAccessUtils.asStack(access);
        }
        Item iron = Items.IRON_INGOT;
        Item circuit = item("mekanism:basic_control_circuit");
        // factory/basic/enriching: ACA / IPI / ACA (A #mekanism:alloys/basic = redstone, C circuit basic, I iron, P enrichment chamber)
        Item basicAlloy = Items.REDSTONE;
        List<ItemStack> factoryPlain = List.of(of(basicAlloy), of(circuit), of(basicAlloy), of(iron), of(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()), of(iron), of(basicAlloy), of(circuit), of(basicAlloy));
        line(lines, level, "factory_plain", "mekanism:factory/basic/enriching", factoryPlain, "a plain enrichment chamber");
        line(lines, level, "factory_configured", "mekanism:factory/basic/enriching",
              List.of(of(basicAlloy), of(circuit), of(basicAlloy), of(iron), configured, of(iron), of(basicAlloy), of(circuit), of(basicAlloy)),
              "a chamber carrying the side_config and ejector components (collectComponents of a configured tile): are they copied to the factory?");
        line(lines, level, "factory_configured_energy", "mekanism:factory/basic/enriching",
              List.of(of(basicAlloy), of(circuit), of(basicAlloy), of(iron), configuredWithEnergy, of(iron), of(basicAlloy), of(circuit), of(basicAlloy)),
              "the same chamber with 1234 energy in the item");
        write("mek_data_assemble", lines);
    }
}
