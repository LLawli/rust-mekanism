package mekanism.common.tests.network;

import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.recipes.ItemStackToItemStackRecipe;
import mekanism.api.recipes.basic.BasicEnrichingRecipe;
import mekanism.api.recipes.book.EnrichingBookCategory;
import mekanism.api.recipes.book.EnrichingBookCategory.EnrichingBookInfo;
import mekanism.api.recipes.ingredients.ItemStackIngredient;
import mekanism.api.recipes.ingredients.creator.IngredientCreatorAccess;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.recipe.lookup.cache.InputRecipeCache;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.neoforged.neoforge.common.crafting.CompoundIngredient;
import net.neoforged.neoforge.common.crafting.DataComponentIngredient;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava `recipe_components.jsonl`: o `DataComponentIngredient` do NeoForge (`neoforge:components`, com `strict` verdadeiro e falso) como uma receita de data
/// pack o escreve, e a ordem das receitas complexas dos caches de entrada.
///
/// Linhas `matrix`: um ingrediente em JSON (o do arquivo da receita) contra uma pilha em JSON (`id`, `count`, `components` no formato do `DataComponentPatch`),
/// com `ItemStackIngredient.test`, `testType` e como `ItemInputCache.mapIngredient` o classifica (`types`, `exact` ou `complex`).
/// Linhas `cache`: receitas (ingrediente em JSON, quantidade, id e o `System.identityHashCode` da chave do id, que é o `RecipeHolder.hashCode`) num
/// `InputRecipeCache.SingleItem`, e as respostas de `findFirstRecipe`, `containsInput` e `findTypeBasedRecipeHolder` para cada pilha.
@ForEachTest(groups = "trace")
public class ComponentIngredientTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final String KIND = "\"neoforge:ingredient_type\":\"neoforge:components\"";

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the NeoForge component ingredient against stacks, and the order of the complex recipes of an input cache.")
    public static void componentIngredients(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(1, 1, 1).fill(0, 0, 0, 0, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> run(helper.getLevel().registryAccess()))
              .thenSucceed());
    }

    private static RegistryOps<JsonElement> ops;
    private static final List<String> lines = new ArrayList<>();

    private static String ingredientJson(String items, String components, boolean strict) {
        return "{" + KIND + ",\"items\":" + items + ",\"components\":" + components + (strict ? ",\"strict\":true" : "") + "}";
    }

    private static String nested(String type, String fields) {
        return "{\"neoforge:ingredient_type\":\"neoforge:" + type + "\"," + fields + "}";
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
    }

    private static Ingredient ingredient(String json) {
        return Ingredient.CODEC.parse(ops, JsonParser.parseString(json)).getOrThrow();
    }

    /// `{"id": "minecraft:coal", "count": 1, "components": {...}}`.
    private static ItemStack stack(String json) {
        com.google.gson.JsonObject object = JsonParser.parseString(json).getAsJsonObject();
        ItemStack stack = new ItemStack(item(object.get("id").getAsString()), object.get("count").getAsInt());
        if (object.has("components")) {
            stack.applyComponents(DataComponentPatch.CODEC.parse(ops, object.get("components")).getOrThrow());
        }
        return stack;
    }

    /// Todos os componentes da pilha, os padrão do item e o patch, como o `DataComponentMap.CODEC` os escreve.
    private static String full(ItemStack stack) {
        return net.minecraft.core.component.DataComponentMap.CODEC.encodeStart(ops, stack.getComponents()).getOrThrow().toString();
    }

    private static String shape(Ingredient ingredient) {
        if (!ingredient.isCustom() || ingredient.isSimple()) {
            return "types";
        }
        if (ingredient.getCustomIngredient() instanceof CompoundIngredient) {
            return "compound";
        }
        if (ingredient.getCustomIngredient() instanceof DataComponentIngredient component && component.componentsExhaustive()) {
            return "exact";
        }
        return "complex";
    }

    private static String quote(String text) {
        return text == null ? "null" : "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static void run(RegistryAccess registry) {
        ops = RegistryOps.create(JsonOps.INSTANCE, registry);
        lines.clear();
        String sword = "\"minecraft:diamond_sword\"";
        String coal = "\"minecraft:coal\"";
        // O strict "de uma pilha": `DataComponentIngredient.of(true, stack)` guarda todos os componentes da pilha, os padrão também.
        String strictCoal = Ingredient.CODEC.encodeStart(ops, DataComponentIngredient.of(true, new ItemStack(Items.COAL))).getOrThrow().toString();
        ItemStack costly = new ItemStack(Items.COAL);
        costly.set(net.minecraft.core.component.DataComponents.REPAIR_COST, 5);
        String strictCoalCost = Ingredient.CODEC.encodeStart(ops, DataComponentIngredient.of(true, costly)).getOrThrow().toString();
        List<String> ingredients = List.of(
              ingredientJson(coal, "{}", false),
              ingredientJson(coal, "{}", true),
              ingredientJson(coal, "{\"minecraft:repair_cost\":5}", false),
              ingredientJson(coal, "{\"minecraft:repair_cost\":5}", true),
              ingredientJson(coal, "{\"minecraft:repair_cost\":0}", false),
              ingredientJson(coal, "{\"minecraft:max_stack_size\":64}", false),
              ingredientJson(coal, "{\"minecraft:max_stack_size\":16}", false),
              ingredientJson(coal, "{\"minecraft:custom_name\":\"Foo\"}", false),
              ingredientJson(coal, "{\"minecraft:custom_name\":\"Foo\"}", true),
              ingredientJson(coal, "{\"minecraft:custom_data\":{\"a\":1}}", false),
              ingredientJson(coal, "{\"minecraft:enchantments\":{}}", false),
              ingredientJson(coal, "{\"minecraft:enchantments\":{\"minecraft:sharpness\":1}}", false),
              ingredientJson(coal, "{\"!minecraft:rarity\":{}}", false),
              ingredientJson(coal, "{\"!minecraft:custom_name\":{}}", false),
              ingredientJson("[\"minecraft:coal\",\"minecraft:charcoal\"]", "{\"minecraft:repair_cost\":5}", false),
              ingredientJson(sword, "{\"minecraft:damage\":3}", false),
              ingredientJson(sword, "{\"minecraft:damage\":0}", false),
              strictCoal,
              strictCoalCost,
              // Os ingredientes em volta de um de componentes: o que o cozimento do jogo (Smelter) e as receitas de data pack escrevem.
              nested("compound", "\"children\":[" + ingredientJson(coal, "{\"minecraft:repair_cost\":5}", false) + ",\"minecraft:bone\"]"),
              nested("compound", "\"children\":[" + ingredientJson(coal, "{\"minecraft:repair_cost\":5}", true) + ",\"minecraft:bone\"]"),
              nested("intersection", "\"children\":[[\"minecraft:coal\",\"minecraft:charcoal\"]," + ingredientJson("[\"minecraft:coal\",\"minecraft:charcoal\"]", "{\"minecraft:repair_cost\":5}", false) + "]"),
              nested("difference", "\"base\":" + ingredientJson("[\"minecraft:coal\",\"minecraft:charcoal\"]", "{}", false) + ",\"subtracted\":" + ingredientJson(coal, "{\"minecraft:repair_cost\":5}", false)));
        List<String> stacks = List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":6}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:custom_name\":\"Foo\"}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:custom_name\":\"Bar\"}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:custom_data\":{\"a\":1}}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:custom_data\":{\"a\":2}}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:enchantments\":{\"minecraft:sharpness\":1}}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:max_stack_size\":16}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"!minecraft:rarity\":{}}}",
              "{\"id\":\"minecraft:charcoal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}",
              "{\"id\":\"minecraft:charcoal\",\"count\":1}",
              "{\"id\":\"minecraft:sand\",\"count\":1}",
              "{\"id\":\"minecraft:bone\",\"count\":1}",
              "{\"id\":\"minecraft:diamond_sword\",\"count\":1}",
              "{\"id\":\"minecraft:diamond_sword\",\"count\":1,\"components\":{\"minecraft:damage\":3}}");
        for (String json : ingredients) {
            Ingredient ingredient = ingredient(json);
            ItemStackIngredient sized = IngredientCreatorAccess.item().from(ingredient, 1);
            for (String stackJson : stacks) {
                ItemStack stack = stack(stackJson);
                lines.add("{\"kind\":\"matrix\",\"ingredient\":" + json + ",\"stack\":" + stackJson + ",\"full\":" + full(stack) + ",\"shape\":\"" + shape(ingredient) + "\",\"test\":" + sized.test(stack)
                      + ",\"test_type\":" + sized.testType(stack) + "}");
            }
        }
        // O strict de pilha, no cache: o `fromPatch` guarda os componentes padrão como patch, a pilha de verdade não.
        cache("strict_defaults", List.of(new Recipe(strictCoal)), List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}"));
        cache("strict_defaults_cost", List.of(new Recipe(strictCoalCost)), List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}"));
        cache("strict_written", List.of(
              new Recipe("a_plain", "\"minecraft:coal\""),
              new Recipe("b_strict", ingredientJson(coal, "{\"minecraft:repair_cost\":5}", true)),
              new Recipe("c_strict_empty", ingredientJson(coal, "{}", true))), List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":6}}"));
        cache("loose_before_plain", List.of(
              new Recipe("a_loose", ingredientJson(coal, "{\"minecraft:repair_cost\":5}", false)),
              new Recipe("b_plain", "\"minecraft:coal\"")), List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}"));
        // Um composto com um filho exato e outro simples: o exato vai para o índice de componentes, o simples para o de tipos.
        cache("compound_strict_child", List.of(
              new Recipe("c0", nested("compound", "\"children\":[" + ingredientJson(coal, "{\"minecraft:repair_cost\":5}", true) + ",\"minecraft:bone\"]")),
              new Recipe("p1", "\"minecraft:coal\"")), List.of(
              "{\"id\":\"minecraft:coal\",\"count\":1}",
              "{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}",
              "{\"id\":\"minecraft:bone\",\"count\":1}"));
        // Receitas complexas que casam a mesma entrada: o `HashSet` delas itera pela ordem de hash, e o hash da chave do id é o de identidade.
        List<String> pool = List.of("minecraft:coal", "minecraft:charcoal", "minecraft:sand", "minecraft:stick", "minecraft:flint", "minecraft:clay_ball",
              "minecraft:bone", "minecraft:string", "minecraft:feather", "minecraft:gunpowder", "minecraft:apple", "minecraft:brick");
        List<Recipe> many = new ArrayList<>();
        List<String> probes = new ArrayList<>();
        for (String id : pool) {
            for (int k = 0; k < 4; k++) {
                many.add(new Recipe(id.substring("minecraft:".length()) + "_" + k, ingredientJson("\"" + id + "\"", "{\"minecraft:repair_cost\":5}", false)));
            }
            probes.add("{\"id\":\"" + id + "\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}");
        }
        cache("complex_hash_order", many, probes);
        // O mesmo com poucas receitas (a tabela de 16 não cresce) e com um balde só de dois.
        List<Recipe> few = new ArrayList<>();
        for (int k = 0; k < 6; k++) {
            few.add(new Recipe("few_" + k, ingredientJson(coal, "{\"minecraft:repair_cost\":5}", false)));
        }
        cache("complex_hash_order_few", few, List.of("{\"id\":\"minecraft:coal\",\"count\":1,\"components\":{\"minecraft:repair_cost\":5}}"));
        try {
            Path file = Path.of("trace", "recipe_components.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private record Recipe(String id, String ingredient) {

        Recipe(String ingredient) {
            this("r0", ingredient);
        }
    }

    private static void cache(String name, List<Recipe> recipes, List<String> stacks) {
        List<RecipeHolder<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>>> holders = new ArrayList<>();
        List<String> recipeJson = new ArrayList<>();
        for (Recipe recipe : recipes) {
            ItemStackIngredient ingredient = IngredientCreatorAccess.item().from(ingredient(recipe.ingredient), 1);
            ResourceKey<net.minecraft.world.item.crafting.Recipe<?>> key = ResourceKey.create(Registries.RECIPE, Identifier.fromNamespaceAndPath("trace", recipe.id));
            holders.add(new RecipeHolder<>(key, new BasicEnrichingRecipe(new net.minecraft.world.item.crafting.Recipe.CommonInfo(true),
                  new EnrichingBookInfo(EnrichingBookCategory.MISC, ""), ingredient, 0F, new ItemStackTemplate(Items.STONE))));
            recipeJson.add("{\"id\":\"" + recipe.id + "\",\"hash\":" + System.identityHashCode(key) + ",\"ingredient\":" + recipe.ingredient + ",\"count\":1}");
        }
        InputRecipeCache.SingleItem<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>> cache = new InputRecipeCache.SingleItem<>(
              MekanismRecipeType.ENRICHING.get(), ItemStackToItemStackRecipe::getInput) {
            {
                initCache(holders);
                initialized = true;
            }
        };
        List<String> queries = new ArrayList<>();
        for (String stackJson : stacks) {
            ItemStack stack = stack(stackJson);
            ItemResource resource = ItemResource.of(stack);
            String head = "{\"stack\":" + stackJson;
            queries.add(head + ",\"op\":\"find_first\",\"result\":" + quote(idOf(holders, cache.findFirstRecipe(null, stack))) + "}");
            queries.add(head + ",\"op\":\"contains\",\"result\":" + cache.containsInput(null, resource) + "}");
            queries.add(head + ",\"op\":\"type_based\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, resource))) + "}");
            queries.add(head + ",\"op\":\"contains_stack\",\"result\":" + cache.containsInput(null, stack) + "}");
            queries.add(head + ",\"op\":\"type_based_stack\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, stack))) + "}");
        }
        lines.add("{\"kind\":\"cache\",\"case\":\"" + name + "\",\"recipes\":[" + String.join(",", recipeJson) + "],\"queries\":[" + String.join(",", queries) + "]}");
    }

    private static String idOf(List<RecipeHolder<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>>> holders,
          RecipeHolder<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>> found) {
        if (found == null) {
            return null;
        }
        for (RecipeHolder<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>> holder : holders) {
            if (holder == found) {
                return holder.id().identifier().getPath();
            }
        }
        throw new IllegalStateException("a receita achada não está na lista");
    }
}
