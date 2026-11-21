package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;
import java.util.function.Supplier;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.ChemicalStack;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.recipes.display.recipe.MekanismRecipeDisplay;
import mekanism.api.recipes.ingredients.chemical.display.ChemicalStackContentsFactory;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.context.ContextMap;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeManager;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.display.FurnaceRecipeDisplay;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;
import net.minecraft.world.item.crafting.display.SlotDisplay;
import net.minecraft.world.item.crafting.display.SlotDisplayContext;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que a colocação da receita pelo livro faz no servidor (`MekanismContainer.handlePlacement`, o que o
/// `ServerGamePacketListenerImpl.handlePlaceRecipe` chama) nas quatro máquinas portadas:
/// - `recipe_place_<máquina>.jsonl`: de cada cenário, o inventário do jogador, os slots de entrada e de saída e o tanque antes, o
///   `hashCode` do `ItemResource` de cada item do inventário (a ordem do `HashMap` do `RecipeIngredientResolver` sai dele), a resposta
///   (`NOTHING`, `PLACE_GHOST_RECIPE` ou a exceção), o mesmo estado depois e o que caiu no chão;
/// - `recipe_place_info.jsonl`: de cada display dos quatro tipos, o `placementInfo().isImpossibleToPlace()` da receita (o que o pacote
///   confere antes), a classe dela e os químicos que cada ingrediente e cada saída mostram (o fantasma da barra do Infuser).
@ForEachTest(groups = "trace")
public class RecipePlaceTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 1, 1);
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;

    private record Spec(String name, Supplier<ItemLike> item, Supplier<ContainerTypeRegistryObject<?>> menu, int input, int output) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismContainerTypes.ENRICHMENT_CHAMBER, 0, 1);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), () -> MekanismContainerTypes.CRUSHER, 0, 1);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(),
          () -> MekanismContainerTypes.ENERGIZED_SMELTER, 0, 1);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismContainerTypes.METALLURGIC_INFUSER, 1, 2);

    /// Um cenário: `display|max|creative|selected|inventário|entrada|saída|tanque`. O inventário é `slot:item:quantidade[:n]` (o `n` dá um
    /// nome à pilha; o slot 40 é a outra mão), a entrada e a saída `item:quantidade`, o tanque `químico:quantidade`.
    private static final String DIRT = "0:dirt:64,1:dirt:64,3:dirt:64,4:dirt:64,5:dirt:64,6:dirt:64,7:dirt:64,8:dirt:64,9:dirt:64,10:dirt:64,11:dirt:64,"
          + "12:dirt:64,13:dirt:64,14:dirt:64,15:dirt:64,16:dirt:64,17:dirt:64,18:dirt:64,19:dirt:64,20:dirt:64,21:dirt:64,22:dirt:64,23:dirt:64,"
          + "24:dirt:64,25:dirt:64,26:dirt:64,27:dirt:64,28:dirt:64,29:dirt:64,30:dirt:64,31:dirt:64,32:dirt:64,33:dirt:64,34:dirt:64,35:dirt:64";

    private static final String[] CHAMBER_CASES = {
          "3913|0|0|0|0:raw_iron:2,10:raw_iron:5|||",
          "3913|1|0|0|0:raw_iron:2,10:raw_iron:5|||",
          "3913|1|0|0|0:raw_iron:64,5:raw_iron:64,20:raw_iron:10|||",
          "3913|0|0|0|3:raw_iron:2|||",
          "3913|1|0|0|3:raw_iron:2|||",
          "3913|0|0|0||||",
          "3913|0|0|0|4:raw_iron:10|raw_iron:1||",
          "3913|0|0|0|4:raw_iron:10|raw_iron:3||",
          "3913|0|0|0|4:raw_iron:5|raw_iron:10||",
          "3913|0|0|0||raw_iron:10||",
          "3913|0|0|0||raw_iron:3||",
          "3913|1|0|0|4:raw_iron:5|raw_iron:10||",
          "3913|1|0|0||raw_iron:10||",
          "3913|1|0|0|4:raw_iron:64,5:raw_iron:64|raw_iron:64||",
          "3913|0|0|0|2:raw_iron:9|raw_gold:6||",
          "3913|1|0|0|2:raw_iron:9|raw_gold:6||",
          "3913|0|0|0|2:raw_iron:9,7:raw_gold:60,30:raw_gold:63|raw_gold:6||",
          "3913|0|0|0|2:raw_iron:9," + DIRT + "|raw_gold:6||",
          "3913|0|1|0|2:raw_iron:9," + DIRT + "|raw_gold:6||",
          "3913|0|0|0|2:raw_iron:3," + DIRT + "|raw_gold:6||",
          "3913|0|1|0|2:raw_iron:3," + DIRT + "|raw_gold:6||",
          "3913|0|0|0|2:raw_iron:9,7:raw_gold:62," + DIRT.replace(",7:dirt:64,", ",") + "|raw_gold:6||",
          "3913|0|0|0|3:raw_iron:2||mekanism:dust_iron:4|",
          "3913|0|0|0|4:raw_iron:10||mekanism:dust_iron:4|",
          "3913|0|0|0|3:raw_iron:2|raw_gold:5|mekanism:dust_iron:4|",
          "3913|0|0|0|2:dirt:64," + DIRT + "|raw_gold:5|mekanism:dust_iron:4|",
          "3913|0|1|0|2:dirt:64," + DIRT + "|raw_gold:5|mekanism:dust_iron:4|",
          "3913|0|0|0|2:raw_gold:62," + DIRT + "|raw_gold:5|mekanism:dust_iron:4|",
          "3913|0|1|3|3:raw_gold:60,40:raw_gold:10,6:raw_iron:5|raw_gold:20||",
          "3913|0|0|3|3:raw_gold:60,40:raw_gold:10,6:raw_iron:5|raw_gold:20||",
          "3913|0|1|3|40:raw_gold:10,6:raw_iron:5,0:raw_gold:1|raw_gold:20||",
          "3913|0|0|0|0:raw_iron:3:n|||",
          "3913|0|0|0|0:raw_iron:3:n,1:raw_iron:3|||",
          "3913|1|0|0|0:raw_iron:3:n,1:raw_iron:5|||",
          "3913|0|0|0|40:raw_iron:10|||",
          "1101|0|0|0|0:mekanism:sawdust:9|||",
          "1101|0|0|0|0:mekanism:sawdust:7|||",
          "1101|1|0|0|0:mekanism:sawdust:7|||",
          "1162|0|0|0|0:sand:5,1:red_sand:5|||",
          "1162|0|0|0|0:red_sand:5,1:sand:5|||",
          "1162|1|0|0|0:sand:5,1:red_sand:7|||",
          "1162|1|0|0|0:sand:5,1:red_sand:7|sand:3||",
          "1162|0|0|0|0:sand:5,1:red_sand:7|red_sand:3||",
          "1162|0|0|0|0:sand:5|red_sand:1||",
          "1249|0|0|0|0:dandelion:5,1:golden_dandelion:5,2:wildflowers:5|||",
          "1249|1|0|0|0:dandelion:5,1:golden_dandelion:6,2:wildflowers:7,3:dandelion:1|||",
          "1249|0|0|0|5:wildflowers:5,6:golden_dandelion:5,7:dandelion:5|||",
          "3913|0|0|0|0:sand:5,1:raw_gold:5,2:raw_iron:5,3:dandelion:5|||",
          "3913|1|0|0|0:sand:5,1:raw_gold:5,2:raw_iron:5,3:dandelion:5|sand:2||",
          "3913|0|0|0|0:stick:5,1:diamond_sword:1,2:raw_iron:3|||",
    };

    private static final String[] CRUSHER_CASES = {
          "604|0|0|0|0:moss_carpet:2,1:pale_moss_carpet:5|||",
          "604|0|0|0|0:pale_moss_carpet:2,1:moss_carpet:5|||",
          "604|0|0|0|0:moss_carpet:3,1:pale_moss_carpet:3|||",
          "604|1|0|0|0:moss_carpet:2,1:pale_moss_carpet:5|||",
          "604|1|0|0|0:moss_carpet:1,1:pale_moss_carpet:1|||",
          "604|0|0|0|0:moss_carpet:2,1:pale_moss_carpet:5|moss_carpet:1||",
          "604|0|0|0|0:moss_carpet:2,1:pale_moss_carpet:5|pale_moss_carpet:1||",
          "604|0|0|0|0:moss_carpet:2,1:pale_moss_carpet:5|cookie:8||",
          "749|0|0|0|8:prismarine_brick_slab:2|||",
          "749|0|0|0|8:prismarine_brick_slab:1,9:prismarine_brick_slab:1|||",
          "749|1|0|0|8:prismarine_brick_slab:40,9:prismarine_brick_slab:40|||",
          "583|0|0|0|8:cookie:4||mekanism:bio_fuel:60|",
          "583|0|0|0|8:cookie:3||mekanism:bio_fuel:60|",
          "583|0|1|0|8:cookie:3||mekanism:bio_fuel:60|",
    };

    private static final String[] SMELTER_CASES = {
          "31|0|0|0|0:potato:5|||",
          "31|1|0|0|0:potato:5|||",
          "31|0|0|0||||",
          "31|0|0|0|0:potato:5|raw_iron:4|iron_ingot:2|",
          "252|0|0|0|0:oak_log:5,1:birch_log:5|||",
    };

    private static final String[] INFUSER_CASES = {
          "1838|0|0|0|0:copper_ingot:5|||",
          "1838|0|0|0|0:copper_ingot:5|||redstone:100",
          "1838|1|0|0|0:copper_ingot:5|||redstone:100",
          "1838|0|0|0|0:copper_ingot:5|||redstone:5",
          "1838|0|0|0|0:copper_ingot:5|||redstone:10",
          "1838|0|0|0|0:copper_ingot:5|||redstone:9",
          "1838|0|0|0|0:copper_ingot:5|||tin:100",
          "1838|0|0|0|0:copper_ingot:5|iron_ingot:3||redstone:100",
          "1838|0|0|0|0:copper_ingot:5|iron_ingot:3||carbon:100",
          "1838|0|0|0|0:copper_ingot:5|iron_ingot:3|mekanism:alloy_infused:2|",
          "1838|0|0|0||copper_ingot:5||redstone:100",
          "1838|0|0|0|0:copper_ingot:5||mekanism:alloy_infused:2|redstone:100",
          "3829|0|0|0|0:copper_ingot:5|||tin:100",
          "3829|0|0|0|0:copper_ingot:2|||tin:100",
          "3829|0|0|0|0:copper_ingot:5|||redstone:100",
          "3829|1|0|0|0:copper_ingot:5|copper_ingot:1||tin:100",
          "3914|0|0|0|0:iron_ingot:5|||carbon:100",
          "3914|0|0|0|0:iron_ingot:5|copper_ingot:2||carbon:100",
          "3914|0|1|0|0:iron_ingot:5|copper_ingot:2||",
    };

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_recipe_place")
    @TestHolder(description = "Records what placing a recipe from the recipe book does in the Enrichment Chamber, and the placement info of every display.")
    public static void chamberPlace(final DynamicTest test) {
        place(test, CHAMBER, CHAMBER_CASES, true);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_recipe_place")
    @TestHolder(description = "Records what placing a recipe from the recipe book does in the Crusher.")
    public static void crusherPlace(final DynamicTest test) {
        place(test, CRUSHER, CRUSHER_CASES, false);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_recipe_place")
    @TestHolder(description = "Records what placing a recipe from the recipe book does in the Energized Smelter.")
    public static void smelterPlace(final DynamicTest test) {
        place(test, SMELTER, SMELTER_CASES, false);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_recipe_place")
    @TestHolder(description = "Records what placing a recipe from the recipe book does in the Metallurgic Infuser.")
    public static void infuserPlace(final DynamicTest test) {
        place(test, INFUSER, INFUSER_CASES, false);
    }

    private static void place(DynamicTest test, Spec spec, String[] cases, boolean info) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  if (info) {
                      info(helper);
                  }
                  run(helper, spec, cases);
              })
              .thenSucceed());
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        Item item = BuiltInRegistries.ITEM.getValue(key);
        if (item == net.minecraft.world.item.Items.AIR) {
            throw new IllegalStateException("no item " + id);
        }
        return item;
    }

    private static String name(ItemStack stack) {
        return BuiltInRegistries.ITEM.getKey(stack.getItem()).toString() + (stack.has(DataComponents.CUSTOM_NAME) ? "#n" : "");
    }

    /// `item:quantidade` ou `ns:item:quantidade`, com `:n` no fim para a pilha com nome.
    private static ItemStack parse(String text) {
        String[] parts = text.split(":");
        boolean named = parts[parts.length - 1].equals("n");
        int end = named ? parts.length - 1 : parts.length;
        int count = Integer.parseInt(parts[end - 1]);
        String id = String.join(":", java.util.Arrays.copyOfRange(parts, 0, end - 1));
        ItemStack stack = new ItemStack(item(id), count);
        if (named) {
            stack.set(DataComponents.CUSTOM_NAME, Component.literal("x"));
        }
        return stack;
    }

    private static ResourceKey<Chemical> chemical(String name) {
        return switch (name) {
            case "redstone" -> ChemicalIds.REDSTONE;
            case "tin" -> ChemicalIds.TIN;
            case "carbon" -> ChemicalIds.CARBON;
            case "diamond" -> ChemicalIds.DIAMOND;
            default -> throw new IllegalStateException(name);
        };
    }

    private static String state(Player player, TileEntityMekanism tile, Spec spec) {
        StringBuilder inv = new StringBuilder("[");
        for (int i = 0; i <= 40; i++) {
            ItemStack stack = player.getInventory().getItem(i);
            if (!stack.isEmpty()) {
                inv.append(inv.length() == 1 ? "" : ",").append('[').append(i).append(",\"").append(name(stack)).append("\",").append(stack.getCount()).append(']');
            }
        }
        List<IInventorySlot> slots = tile.getInventorySlots();
        ItemStack input = slots.get(spec.input).asItemStack();
        ItemStack output = slots.get(spec.output).asItemStack();
        String tank = "null";
        if (tile instanceof TileEntityMetallurgicInfuser infuser) {
            tank = infuser.infusionTank.isEmpty() ? "[\"\",0]"
                                                 : "[\"" + infuser.infusionTank.resource().toStack(infuser.infusionTank.amountAsInt()).typeHolder().getRegisteredName() + "\"," + infuser.infusionTank.amountAsInt() + "]";
        }
        return "{\"inv\":" + inv + "],\"input\":[\"" + (input.isEmpty() ? "" : name(input)) + "\"," + input.getCount() + "],\"output\":[\""
               + (output.isEmpty() ? "" : name(output)) + "\"," + output.getCount() + "],\"tank\":" + tank + "}";
    }

    private static void run(MekGameTestHelper helper, Spec spec, String[] cases) {
        var level = helper.getLevel();
        RecipeManager manager = level.recipeAccess();
        Player player = helper.makeMockPlayerLookingAt(1, 0, 1, Direction.UP);
        helper.useOn(new BlockPos(1, 0, 1), new ItemStack(spec.item.get()), player, Direction.UP);
        BlockEntity found = level.getBlockEntity(helper.absolutePos(MACHINE));
        TileEntityMekanism tile = (TileEntityMekanism) found;
        MekanismTileContainer<TileEntityMekanism> menu = new MekanismTileContainer<>(spec.menu.get(), 100, player.getInventory(), tile);
        player.containerMenu = menu;
        List<IInventorySlot> slots = tile.getInventorySlots();
        List<String> lines = new ArrayList<>();
        AABB all = player.getBoundingBox().inflate(6);
        for (int index = 0; index < cases.length; index++) {
            String[] parts = cases[index].split("\\|", -1);
            int display = Integer.parseInt(parts[0]);
            boolean max = parts[1].equals("1");
            boolean creative = parts[2].equals("1");
            int selected = Integer.parseInt(parts[3]);
            for (IInventorySlot slot : slots) {
                slot.setContents(ItemResource.EMPTY, 0, null);
            }
            for (int i = 0; i < player.getInventory().getContainerSize(); i++) {
                player.getInventory().setItem(i, ItemStack.EMPTY);
            }
            player.getInventory().setSelectedSlot(selected);
            TreeMap<String, Integer> hashes = new TreeMap<>();
            if (!parts[4].isEmpty()) {
                for (String entry : parts[4].split(",")) {
                    int colon = entry.indexOf(':');
                    ItemStack stack = parse(entry.substring(colon + 1));
                    player.getInventory().setItem(Integer.parseInt(entry.substring(0, colon)), stack);
                    hashes.put(name(stack), ItemResource.of(stack).hashCode());
                }
            }
            if (!parts[5].isEmpty()) {
                ItemStack stack = parse(parts[5]);
                slots.get(spec.input).setContents(ItemResource.of(stack), stack.getCount(), null);
            }
            if (!parts[6].isEmpty()) {
                ItemStack stack = parse(parts[6]);
                slots.get(spec.output).setContents(ItemResource.of(stack), stack.getCount(), null);
            }
            if (tile instanceof TileEntityMetallurgicInfuser infuser) {
                if (parts[7].isEmpty()) {
                    infuser.infusionTank.setContents(ChemicalResource.EMPTY, 0, null);
                } else {
                    String[] tank = parts[7].split(":");
                    Holder<Chemical> chemical = level.registryAccess().getOrThrow(chemical(tank[0]));
                    infuser.infusionTank.setContents(ChemicalResource.of(chemical), Integer.parseInt(tank[1]), null);
                }
            }
            level.getEntitiesOfClass(ItemEntity.class, all).forEach(ItemEntity::discard);
            String before = state(player, tile, spec);
            RecipeHolder<?> holder = manager.getRecipeFromDisplay(new RecipeDisplayId(display)).parent();
            String action;
            try {
                action = menu.handlePlacement(max, creative, holder, level, player.getInventory()).name();
            } catch (Throwable error) {
                action = "error:" + error.getClass().getName();
            }
            StringBuilder dropped = new StringBuilder("[");
            for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, all)) {
                dropped.append(dropped.length() == 1 ? "" : ",").append("[\"").append(name(entity.getItem())).append("\",").append(entity.getItem().getCount()).append(']');
                entity.discard();
            }
            StringBuilder hashText = new StringBuilder("{");
            for (var entry : hashes.entrySet()) {
                hashText.append(hashText.length() == 1 ? "" : ",").append('"').append(entry.getKey()).append("\":").append(entry.getValue());
            }
            lines.add("{\"case\":" + index + ",\"display\":" + display + ",\"recipe\":\"" + holder.id().identifier() + "\",\"max\":" + max + ",\"creative\":" + creative
                      + ",\"selected\":" + selected + ",\"before\":" + before + ",\"hashes\":" + hashText + "},\"action\":\"" + action + "\",\"after\":"
                      + state(player, tile, spec) + ",\"dropped\":" + dropped + "]}");
        }
        write("recipe_place_" + spec.name, lines);
    }

    private static String chemicals(List<SlotDisplay> displays, ContextMap context) {
        StringBuilder out = new StringBuilder("[");
        for (SlotDisplay display : displays) {
            StringBuilder one = new StringBuilder("[");
            for (ChemicalStack stack : display.resolve(context, ChemicalStackContentsFactory.INSTANCE).toList()) {
                one.append(one.length() == 1 ? "" : ",").append("[\"").append(stack.typeHolder().getRegisteredName()).append("\",").append(stack.amount()).append(']');
            }
            out.append(out.length() == 1 ? "" : ",").append(one).append(']');
        }
        return out.append(']').toString();
    }

    private static void info(MekGameTestHelper helper) {
        var level = helper.getLevel();
        RecipeManager manager = level.recipeAccess();
        ContextMap context = SlotDisplayContext.fromLevel(level);
        List<RecipeType<?>> types = List.of(MekanismRecipeType.ENRICHING.get(), MekanismRecipeType.CRUSHING.get(), RecipeType.SMELTING,
              MekanismRecipeType.METALLURGIC_INFUSING.get());
        List<String> lines = new ArrayList<>();
        for (int index = 0; manager.getRecipeFromDisplay(new RecipeDisplayId(index)) != null; index++) {
            RecipeManager.ServerDisplayInfo found = manager.getRecipeFromDisplay(new RecipeDisplayId(index));
            RecipeHolder<?> holder = found.parent();
            if (!types.contains(holder.value().getType())) {
                continue;
            }
            String chem = "[]";
            String chemOut = "[]";
            if (found.display().display() instanceof MekanismRecipeDisplay recipeDisplay) {
                chem = chemicals(recipeDisplay.ingredients(), context);
                chemOut = chemicals(recipeDisplay.rawOutputs(), context);
            } else if (found.display().display() instanceof FurnaceRecipeDisplay furnace) {
                chem = chemicals(List.of(furnace.ingredient()), context);
                chemOut = chemicals(List.of(furnace.result()), context);
            }
            lines.add("{\"id\":" + index + ",\"recipe\":\"" + holder.id().identifier() + "\",\"impossible\":" + holder.value().placementInfo().isImpossibleToPlace()
                      + ",\"class\":\"" + holder.value().getClass().getSimpleName() + "\",\"chem\":" + chem + ",\"chem_out\":" + chemOut + "}");
        }
        write("recipe_place_info", lines);
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
}
