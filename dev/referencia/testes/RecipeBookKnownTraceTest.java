package mekanism.common.tests.network;

import com.google.common.collect.Sets;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.client.ClientRecipeBook;
import net.minecraft.client.gui.screens.recipebook.RecipeCollection;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeBookCategory;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeManager;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.display.RecipeDisplayEntry;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// A ordem das coleções do livro de receitas quando o jogador conhece receitas de todo o jogo, não só as das máquinas. Grava:
/// `recipe_book_known_displays.jsonl`: de cada receita do jogo, na ordem de `RecipeManager.getRecipes()` (a de `/recipe give @s *`), o nome e
/// quantos displays ela tem (`listDisplaysForRecipe`; os ids seguem essa ordem, do zero, o que `ascending` confere), e uma linha `header` com os
/// totais;
/// `recipe_book_known_client.jsonl`: cenários com o `ClientRecipeBook` de verdade, que recebe `add`, `remove` e `clear` com displays de qualquer
/// receita (o que os pacotes do servidor fazem) e, a cada `snap`, as coleções das abas das quatro máquinas, na ordem. O cenário `login` monta a
/// ordem de `ServerRecipeBook.sendInitialRecipeBook`: as chaves das receitas entram num `Sets.newIdentityHashSet()` e saem na ordem dele, com o
/// `System.identityHashCode` de cada chave gravado (ele muda a cada execução); a receita vai como o índice dela no outro arquivo.
@ForEachTest(groups = "trace")
public class RecipeBookKnownTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3));
    }

    /// `state * 6364136223846793005 + 1442695040888963407`, os 31 bits de cima: o mesmo gerador no teste em Rust, que não precisa dele (os ids
    /// vão gravados), mas mantém o rastro igual entre execuções.
    private static final class Lcg {

        private long state;

        Lcg(long seed) {
            state = seed;
        }

        int next(int bound) {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) ((state >>> 33) % bound);
        }
    }

    private static List<Integer> shuffled(List<Integer> ids, Lcg random) {
        List<Integer> out = new ArrayList<>(ids);
        for (int i = out.size() - 1; i > 0; i--) {
            int j = random.next(i + 1);
            Integer other = out.get(j);
            out.set(j, out.get(i));
            out.set(i, other);
        }
        return out;
    }

    private static List<Integer> range(int total) {
        List<Integer> out = new ArrayList<>(total);
        for (int i = 0; i < total; i++) {
            out.add(i);
        }
        return out;
    }

    private static String ints(List<Integer> ids) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < ids.size(); i++) {
            out.append(i == 0 ? "" : ",").append(ids.get(i));
        }
        return out.append(']').toString();
    }

    /// Um cenário: o livro, as linhas do rastro e as categorias que os `snap` listam.
    private static final class Scene {

        final ClientRecipeBook book = new ClientRecipeBook();
        final List<RecipeDisplayEntry> displays;
        final Set<RecipeBookCategory> categories;
        final List<String> out;
        final String name;

        Scene(String name, List<RecipeDisplayEntry> displays, Set<RecipeBookCategory> categories, List<String> out) {
            this.name = name;
            this.displays = displays;
            this.categories = categories;
            this.out = out;
        }

        void add(List<Integer> ids) {
            for (int id : ids) {
                book.add(displays.get(id));
            }
            out.add("{\"scene\":\"" + name + "\",\"op\":\"add\",\"ids\":" + ints(ids) + "}");
        }

        void remove(List<Integer> ids) {
            for (int id : ids) {
                book.remove(new RecipeDisplayId(id));
            }
            out.add("{\"scene\":\"" + name + "\",\"op\":\"remove\",\"ids\":" + ints(ids) + "}");
        }

        void clear() {
            book.clear();
            out.add("{\"scene\":\"" + name + "\",\"op\":\"clear\"}");
        }

        void snap() {
            book.rebuildCollections();
            StringBuilder tabs = new StringBuilder();
            int listed = 0;
            for (RecipeBookCategory category : categories) {
                StringBuilder lists = new StringBuilder();
                for (RecipeCollection collection : book.getCollection(category)) {
                    List<Integer> ids = new ArrayList<>();
                    for (RecipeDisplayEntry entry : collection.getRecipes()) {
                        ids.add(entry.id().index());
                        listed++;
                    }
                    lists.append(lists.length() == 0 ? "" : ",").append(ints(ids));
                }
                tabs.append(tabs.length() == 0 ? "" : ",").append('"').append(BuiltInRegistries.RECIPE_BOOK_CATEGORY.getKey(category)).append("\":[").append(lists)
                      .append(']');
            }
            out.add("{\"scene\":\"" + name + "\",\"op\":\"snap\",\"listed\":" + listed + ",\"tabs\":{" + tabs + "}}");
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_book_known")
    @TestHolder(description = "Records the order of the recipe book collections when the player knows displays of every recipe in the game.")
    public static void known(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  RecipeManager manager = helper.getLevel().recipeAccess();
                  int total = 0;
                  while (manager.getRecipeFromDisplay(new RecipeDisplayId(total)) != null) {
                      total++;
                  }
                  List<RecipeDisplayEntry> displays = new ArrayList<>();
                  for (int id = 0; id < total; id++) {
                      displays.add(manager.getRecipeFromDisplay(new RecipeDisplayId(id)).display());
                  }
                  List<RecipeType<?>> types = List.of(MekanismRecipeType.ENRICHING.get(), MekanismRecipeType.CRUSHING.get(), RecipeType.SMELTING,
                        MekanismRecipeType.METALLURGIC_INFUSING.get());
                  Set<RecipeBookCategory> categories = new LinkedHashSet<>();
                  List<Integer> bookIds = new ArrayList<>();
                  List<Integer> otherIds = new ArrayList<>();
                  for (int id = 0; id < total; id++) {
                      RecipeManager.ServerDisplayInfo info = manager.getRecipeFromDisplay(new RecipeDisplayId(id));
                      if (types.contains(info.parent().value().getType())) {
                          categories.add(info.display().category());
                          bookIds.add(id);
                      } else {
                          otherIds.add(id);
                      }
                  }
                  // As receitas, na ordem de `getRecipes()`.
                  List<String> lines = new ArrayList<>();
                  List<RecipeHolder<?>> holders = new ArrayList<>(manager.getRecipes());
                  int next = 0;
                  boolean ascending = true;
                  for (RecipeHolder<?> holder : holders) {
                      List<Integer> ids = new ArrayList<>();
                      manager.listDisplaysForRecipe(holder.id(), display -> ids.add(display.id().index()));
                      for (int id : ids) {
                          ascending &= id == next;
                          next++;
                      }
                      lines.add("{\"recipe\":\"" + holder.id().identifier() + "\",\"n\":" + ids.size() + "}");
                  }
                  lines.add(0, "{\"header\":true,\"displays\":" + total + ",\"recipes\":" + holders.size() + ",\"ascending\":" + ascending + ",\"book\":"
                        + bookIds.size() + "}");
                  write("recipe_book_known_displays", lines);

                  List<String> out = new ArrayList<>();
                  try {
                      scenes(out, displays, categories, total, bookIds, otherIds, holders, manager);
                  } catch (Throwable e) {
                      out.add("{\"error\":\"" + e.toString().replace("\"", "'") + "\"}");
                  }
                  write("recipe_book_known_client", out);
              })
              .thenSucceed()
        );
    }

    private static void scenes(List<String> out, List<RecipeDisplayEntry> displays, Set<RecipeBookCategory> categories, int total, List<Integer> bookIds,
          List<Integer> otherIds, List<RecipeHolder<?>> holders, RecipeManager manager) {
        List<Integer> all = range(total);

        // `/recipe give @s *` num livro novo: tudo, em ordem de id.
        Scene scene = new Scene("all_ascending", displays, categories, out);
        scene.add(all);
        scene.snap();

        // Tudo, em ordem qualquer.
        scene = new Scene("all_shuffled", displays, categories, out);
        scene.add(shuffled(all, new Lcg(11)));
        scene.snap();

        // Metade: a tabela para em 4.096 e os ids que diferem de 4.096 dividem o balde, na ordem em que chegaram. Depois saem 1.200 e entram 800.
        scene = new Scene("partial_2500", displays, categories, out);
        List<Integer> order = shuffled(all, new Lcg(22));
        scene.add(order.subList(0, 2500));
        scene.snap();
        scene.remove(shuffled(order.subList(0, 2500), new Lcg(23)).subList(0, 1200));
        scene.snap();
        scene.add(order.subList(2500, 3300));
        scene.snap();

        // 900 (tabela de 2.048), e mais 700 (a tabela dobra e cada balde se parte).
        scene = new Scene("partial_900", displays, categories, out);
        order = shuffled(all, new Lcg(33));
        scene.add(order.subList(0, 900));
        scene.snap();
        scene.add(order.subList(900, 1600));
        scene.snap();

        // Poucas: 150 de todo o jogo (tabela de 256).
        scene = new Scene("small_150", displays, categories, out);
        scene.add(shuffled(all, new Lcg(44)).subList(0, 150));
        scene.snap();

        // Muitas do livro das máquinas e poucas do resto: 300 e 100, misturadas (tabela de 1.024).
        scene = new Scene("book_heavy", displays, categories, out);
        List<Integer> mixed = new ArrayList<>(shuffled(bookIds, new Lcg(55)).subList(0, 300));
        mixed.addAll(shuffled(otherIds, new Lcg(56)).subList(0, 100));
        scene.add(shuffled(mixed, new Lcg(57)));
        scene.snap();

        // Só as do livro das máquinas (o que o port contava antes): 614, em ordem de id.
        scene = new Scene("book_only", displays, categories, out);
        scene.add(bookIds);
        scene.snap();

        // O pacote com `replace` (`clear` e `add`): a tabela não encolhe.
        scene = new Scene("replace", displays, categories, out);
        scene.add(all);
        scene.clear();
        scene.add(shuffled(all, new Lcg(66)).subList(0, 400));
        scene.snap();

        // A entrada no mundo: `ServerRecipeBook.sendInitialRecipeBook` percorre o `IdentityHashSet` das chaves.
        for (int size : new int[] {40, 700, 2600}) {
            scene = new Scene("login_" + size, displays, categories, out);
            List<Integer> picks = shuffled(range(holders.size()), new Lcg(77 + size)).subList(0, size);
            Set<ResourceKey<Recipe<?>>> known = Sets.newIdentityHashSet();
            java.util.Map<ResourceKey<Recipe<?>>, Integer> indexOf = new java.util.IdentityHashMap<>();
            for (int pick : picks) {
                indexOf.put(holders.get(pick).id(), pick);
            }
            StringBuilder inserted = new StringBuilder();
            for (int pick : picks) {
                ResourceKey<Recipe<?>> key = holders.get(pick).id();
                known.add(key);
                inserted.append(inserted.length() == 0 ? "" : ",").append('[').append(pick).append(',').append(System.identityHashCode(key)).append(']');
            }
            StringBuilder iterated = new StringBuilder();
            List<Integer> ids = new ArrayList<>();
            for (ResourceKey<Recipe<?>> key : known) {
                iterated.append(iterated.length() == 0 ? "" : ",").append(indexOf.get(key));
                manager.listDisplaysForRecipe(key, display -> ids.add(display.id().index()));
            }
            out.add("{\"scene\":\"" + scene.name + "\",\"op\":\"identity\",\"inserted\":[" + inserted + "],\"iterated\":[" + iterated + "]}");
            scene.add(ids);
            scene.snap();
        }
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
