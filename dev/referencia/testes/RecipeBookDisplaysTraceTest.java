package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registration.impl.RecipeBookCategoryDeferredRegister.MekanismRecipeBookTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.ChatFormatting;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.client.ClientRecipeBook;
import net.minecraft.client.gui.screens.recipebook.RecipeCollection;
import net.minecraft.client.searchtree.FullTextSearchTree;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.crafting.RecipeBookCategory;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeManager;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.display.RecipeDisplayEntry;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;
import net.minecraft.world.item.crafting.display.SlotDisplayContext;
import net.minecraft.util.context.ContextMap;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava, das receitas dos quatro tipos que as máquinas portadas usam (`enriching`, `crushing`, `smelting` e `metallurgic_infusing`), o que
/// o servidor manda ao livro de receitas do jogador: cada `RecipeDisplayEntry` com o id do display (o índice na lista de todos os displays
/// do jogo, `RecipeManager.unpackRecipeInfo`), o grupo, a categoria do livro, a classe do display, o resultado e as linhas de tooltip dele (o que a
/// busca do livro lê), e as categorias de cada tipo (as abas do livro). Uma linha `header` com o total de displays e uma por tipo com as abas.
@ForEachTest(groups = "trace")
public class RecipeBookDisplaysTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_book")
    @TestHolder(description = "Records the recipe book displays of the recipe types the ported machines use, as the server sends them to the player's book.")
    public static void displays(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  var level = helper.getLevel();
                  RecipeManager manager = level.recipeAccess();
                  ContextMap context = SlotDisplayContext.fromLevel(level);
                  Item.TooltipContext tooltipContext = Item.TooltipContext.of(level.registryAccess());
                  List<String> lines = new ArrayList<>();
                  List<RecipeType<?>> types = List.of(MekanismRecipeType.ENRICHING.get(), MekanismRecipeType.CRUSHING.get(),
                        RecipeType.SMELTING, MekanismRecipeType.METALLURGIC_INFUSING.get());
                  List<String> names = List.of("enriching", "crushing", "smelting", "metallurgic_infusing");
                  List<MekanismRecipeBookTypes> books = List.of(MekanismRecipeType.ENRICHING.getRecipeType().getBookCategories(), MekanismRecipeType.CRUSHING.getRecipeType().getBookCategories(),
                        MekanismRecipeType.SMELTING.getRecipeType().getBookCategories(), MekanismRecipeType.METALLURGIC_INFUSING.getRecipeType().getBookCategories());
                  int total = 0;
                  while (manager.getRecipeFromDisplay(new RecipeDisplayId(total)) != null) {
                      total++;
                  }
                  lines.add("{\"header\":true,\"displays\":" + total + "}");
                  for (int t = 0; t < types.size(); t++) {
                      StringBuilder tabs = new StringBuilder();
                      for (var category : books.get(t).categories()) {
                          tabs.append(tabs.length() == 0 ? "" : ",").append("{\"name\":\"").append(category.getSerializedName()).append("\",\"book\":\"")
                                .append(BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey(category.toRecipeBookCategory())).append("\"}");
                      }
                      lines.add("{\"type\":\"" + names.get(t) + "\",\"tabs\":[" + tabs + "]}");
                  }
                  List<RecipeDisplayEntry> entries = new ArrayList<>();
                  for (int index = 0; index < total; index++) {
                      RecipeManager.ServerDisplayInfo info = manager.getRecipeFromDisplay(new RecipeDisplayId(index));
                      RecipeHolder<?> holder = info.parent();
                      int which = types.indexOf(holder.value().getType());
                      if (which < 0) {
                          continue;
                      }
                      RecipeDisplayEntry entry = info.display();
                      entries.add(entry);
                      StringBuilder results = new StringBuilder();
                      StringBuilder tips = new StringBuilder();
                      for (ItemStack stack : entry.resultItems(context)) {
                          results.append(results.length() == 0 ? "" : ",").append("{\"item\":\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem()))
                                .append("\",\"count\":").append(stack.getCount()).append("}");
                          for (Component line : stack.getTooltipLines(tooltipContext, null, TooltipFlag.Default.NORMAL)) {
                              String text = ChatFormatting.stripFormatting(line.getString()).trim();
                              if (!text.isEmpty()) {
                                  tips.append(tips.length() == 0 ? "" : ",").append('"').append(text.replace("\\", "\\\\").replace("\"", "\\\"")).append('"');
                              }
                          }
                      }
                      lines.add("{\"id\":" + index + ",\"recipe\":\"" + holder.id().identifier() + "\",\"type\":\"" + names.get(which) + "\",\"group\":"
                            + (entry.group().isPresent() ? String.valueOf(entry.group().getAsInt()) : "null") + ",\"category\":\""
                            + BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey(entry.category()) + "\",\"display\":\"" + entry.display().getClass().getSimpleName()
                            + "\",\"requirements\":" + (entry.craftingRequirements().isPresent() ? String.valueOf(entry.craftingRequirements().get().size()) : "null")
                            + ",\"results\":[" + results + "],\"tooltip\":[" + tips + "]}");
                  }
                  write("recipe_book_displays", lines);
                  List<String> client = new ArrayList<>();
                  try {
                      client(client, entries, 1, context, tooltipContext, level.registryAccess());
                      client(client, entries, 3, context, tooltipContext, level.registryAccess());
                  } catch (Throwable e) {
                      client.add("{\"error\":\"" + e.toString().replace("\"", "'") + "\"}");
                  }
                  write("recipe_book_client", client);
              })
              .thenSucceed()
        );
    }

    private static final String[] QUERIES = {"iron", "IRON", "dust", "gold ingot", "e", "minecraft:iron", "mekanism:dust", "mekanism:", ":dust", "mek : dust",
          "zzz", "block", "ore", "clay", "enriched", "stone", "o:", "minecraft: ingot", "  dust", "dust "};

    /// O `ClientRecipeBook` de verdade: as receitas de `entries` (a cada `step`-ésima, na ordem do id) entram, `rebuildCollections` as agrupa, e
    /// a lista de cada categoria e de cada busca sai com os ids. A busca é a `FullTextSearchTree` do `SessionSearchTrees`, com as linhas de
    /// tooltip do resultado (sem formatação e sem espaço nas pontas, as vazias fora) e os ids dele.
    private static void client(List<String> out, List<RecipeDisplayEntry> entries, int step, ContextMap context, Item.TooltipContext tooltipContext,
          net.minecraft.core.RegistryAccess access) {
        ClientRecipeBook book = new ClientRecipeBook();
        int count = 0;
        for (int i = 0; i < entries.size(); i++) {
            if (i % step == 0) {
                book.add(entries.get(i));
                count++;
            }
        }
        book.rebuildCollections();
        java.util.Set<RecipeBookCategory> seen = new java.util.LinkedHashSet<>();
        for (int i = 0; i < entries.size(); i++) {
            if (i % step == 0) {
                seen.add(entries.get(i).category());
            }
        }
        StringBuilder tabs = new StringBuilder();
        for (RecipeBookCategory category : seen) {
            StringBuilder lists = new StringBuilder();
            for (RecipeCollection collection : book.getCollection(category)) {
                StringBuilder ids = new StringBuilder();
                for (RecipeDisplayEntry entry : collection.getRecipes()) {
                    ids.append(ids.length() == 0 ? "" : ",").append(entry.id().index());
                }
                lists.append(lists.length() == 0 ? "" : ",").append('[').append(ids).append(']');
            }
            tabs.append(tabs.length() == 0 ? "" : ",").append('"').append(BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey(category)).append("\":[").append(lists).append(']');
        }
        FullTextSearchTree<RecipeCollection> tree = new FullTextSearchTree<>(
              collection -> collection.getRecipes().stream().flatMap(entry -> entry.resultItems(context).stream()
                    .flatMap(stack -> stack.getTooltipLines(tooltipContext, null, TooltipFlag.Default.NORMAL).stream()))
                    .map(line -> ChatFormatting.stripFormatting(line.getString()).trim()).filter(text -> !text.isEmpty()),
              collection -> collection.getRecipes().stream().flatMap(entry -> entry.resultItems(context).stream().map(stack -> BuiltInRegistries.ITEM.getKey(stack.getItem()))),
              book.getCollections());
        StringBuilder queries = new StringBuilder();
        for (String query : QUERIES) {
            StringBuilder ids = new StringBuilder();
            for (RecipeCollection collection : tree.search(query.toLowerCase(java.util.Locale.ROOT))) {
                ids.append(ids.length() == 0 ? "" : ",").append(collection.getRecipes().get(0).id().index());
            }
            queries.append(queries.length() == 0 ? "" : ",").append("{\"q\":\"").append(query).append("\",\"first_ids\":[").append(ids).append("]}");
        }
        out.add("{\"step\":" + step + ",\"known\":" + count + ",\"tabs\":{" + tabs.toString().replaceFirst("^\"", "\"") + "},\"queries\":[" + queries + "]}");
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
