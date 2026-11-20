package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeSet;
import mekanism.api.recipes.display.recipe.MekanismRecipeDisplay;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.ByteTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.stats.RecipeBookSettings;
import net.minecraft.stats.ServerRecipeBook;
import net.minecraft.util.context.ContextMap;
import net.minecraft.world.entity.player.StackedItemContents;
import net.minecraft.world.inventory.RecipeBookType;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeManager;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.display.FurnaceRecipeDisplay;
import net.minecraft.world.item.crafting.display.RecipeDisplayEntry;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;
import net.minecraft.world.item.crafting.display.SlotDisplay;
import net.minecraft.world.item.crafting.display.SlotDisplayContext;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que o livro de receitas guarda e confere, para os quatro tipos das máquinas portadas:
/// - `recipe_book_requirements.jsonl`: de cada display, os `craftingRequirements` (os itens de cada `Ingredient`), a quantidade que cada
///   ingrediente do `MekanismRecipeDisplay` pede (`resolveForFirstStack(...).count()`), os itens que cada ingrediente e cada saída mostram
///   (`resolveForStacks`, o que o fantasma desenha) e o `showNotification` da receita;
/// - `recipe_book_craft.jsonl`: cenários de inventário (pilhas contadas por `StackedItemContents.accountStack`) com os displays que o
///   `MekanismRecipeBookComponent.canCraft` marca como fazíveis (a lógica dele copiada aqui, sobre o `StackedItemContents` de verdade);
/// - `recipe_book_nbt.jsonl`: o que o `ServerRecipeBook.Packed.CODEC` grava no NBT do jogador (os campos do `RecipeBookSettings` com os
///   tipos de livro de mod do NeoForge, `recipes` e `toBeDisplayed`) e o que ele lê de volta.
@ForEachTest(groups = "trace")
public class RecipeBookStateTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3));
    }

    private static String key(Holder<Item> item) {
        return BuiltInRegistries.ITEM.getKey(item.value()).toString();
    }

    private static String stacks(List<ItemStack> stacks) {
        StringBuilder out = new StringBuilder("[");
        for (ItemStack stack : stacks) {
            out.append(out.length() == 1 ? "" : ",").append("[\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem())).append("\",").append(stack.getCount()).append(']');
        }
        return out.append(']').toString();
    }

    private static String displays(List<SlotDisplay> displays, ContextMap context) {
        StringBuilder out = new StringBuilder("[");
        for (SlotDisplay display : displays) {
            out.append(out.length() == 1 ? "" : ",").append(stacks(display.resolveForStacks(context)));
        }
        return out.append(']').toString();
    }

    /// `MekanismRecipeBookComponent.canCraft`, linha a linha.
    private static boolean canCraft(RecipeDisplayEntry entry, StackedItemContents stackedContents, ContextMap recipeContext) {
        if (entry.craftingRequirements().isPresent()) {
            List<Ingredient> craftingRequirements = entry.craftingRequirements().get();
            if (craftingRequirements.isEmpty()) {
                return true;
            }
            if (entry.display() instanceof MekanismRecipeDisplay recipeDisplay) {
                List<Ingredient> scaledRequirements = new ArrayList<>();
                int itemIndex = 0;
                for (SlotDisplay display : recipeDisplay.ingredients()) {
                    int required = display.resolveForFirstStack(recipeContext).count();
                    if (required > 0) {
                        if (itemIndex >= craftingRequirements.size()) {
                            return false;
                        }
                        Ingredient ingredient = craftingRequirements.get(itemIndex++);
                        for (int i = 0; i < required; i++) {
                            scaledRequirements.add(ingredient);
                        }
                    }
                }
                return stackedContents.canCraft(scaledRequirements, null);
            }
            return stackedContents.canCraft(craftingRequirements, null);
        }
        return false;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_book_state")
    @TestHolder(description = "Records the crafting requirements of the recipe book displays, what canCraft answers for some inventories, and the player's recipe book NBT.")
    public static void state(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  var level = helper.getLevel();
                  RecipeManager manager = level.recipeAccess();
                  ContextMap context = SlotDisplayContext.fromLevel(level);
                  List<RecipeType<?>> types = List.of(MekanismRecipeType.ENRICHING.get(), MekanismRecipeType.CRUSHING.get(),
                        RecipeType.SMELTING, MekanismRecipeType.METALLURGIC_INFUSING.get());
                  List<String> lines = new ArrayList<>();
                  List<RecipeDisplayEntry> entries = new ArrayList<>();
                  List<ResourceKey<Recipe<?>>> keys = new ArrayList<>();
                  TreeSet<String> universe = new TreeSet<>();
                  for (int index = 0; manager.getRecipeFromDisplay(new RecipeDisplayId(index)) != null; index++) {
                      RecipeManager.ServerDisplayInfo info = manager.getRecipeFromDisplay(new RecipeDisplayId(index));
                      RecipeHolder<?> holder = info.parent();
                      if (!types.contains(holder.value().getType())) {
                          continue;
                      }
                      RecipeDisplayEntry entry = info.display();
                      entries.add(entry);
                      keys.add(holder.id());
                      String req = "null";
                      if (entry.craftingRequirements().isPresent()) {
                          StringBuilder all = new StringBuilder("[");
                          for (Ingredient ingredient : entry.craftingRequirements().get()) {
                              TreeSet<String> items = new TreeSet<>();
                              ingredient.items().forEach(item -> items.add(key(item)));
                              universe.addAll(items);
                              StringBuilder one = new StringBuilder("[");
                              for (String item : items) {
                                  one.append(one.length() == 1 ? "" : ",").append('"').append(item).append('"');
                              }
                              all.append(all.length() == 1 ? "" : ",").append(one).append(']');
                          }
                          req = all.append(']').toString();
                      }
                      String counts = "null";
                      String ing;
                      String out;
                      if (entry.display() instanceof MekanismRecipeDisplay recipeDisplay) {
                          StringBuilder all = new StringBuilder("[");
                          for (SlotDisplay display : recipeDisplay.ingredients()) {
                              all.append(all.length() == 1 ? "" : ",").append(display.resolveForFirstStack(context).count());
                          }
                          counts = all.append(']').toString();
                          ing = displays(recipeDisplay.ingredients(), context);
                          out = displays(recipeDisplay.rawOutputs(), context);
                      } else if (entry.display() instanceof FurnaceRecipeDisplay furnace) {
                          ing = displays(List.of(furnace.ingredient()), context);
                          out = displays(List.of(furnace.result()), context);
                      } else {
                          ing = "[]";
                          out = "[]";
                      }
                      lines.add("{\"id\":" + index + ",\"recipe\":\"" + holder.id().identifier() + "\",\"notify\":" + holder.value().showNotification()
                            + ",\"special\":" + holder.value().isSpecial() + ",\"req\":" + req + ",\"counts\":" + counts + ",\"ing\":" + ing + ",\"out\":" + out
                            + ",\"result\":" + stacks(entry.display().result().resolveForStacks(context)) + "}");
                  }
                  write("recipe_book_requirements", lines);

                  // Os cenários de inventário: pilhas sorteadas por um gerador fixo sobre os itens que algum requisito aceita.
                  List<String> items = new ArrayList<>(universe);
                  List<String> craft = new ArrayList<>();
                  long seed = 0x5DEECE66DL;
                  int[] sizes = {0, 1, 3, 8, 20, 36, 36, 36, 12, 12};
                  int[] caps = {64, 64, 64, 64, 64, 64, 1, 2, 3, 64};
                  for (int scenario = 0; scenario < sizes.length; scenario++) {
                      StackedItemContents contents = new StackedItemContents();
                      StringBuilder dumped = new StringBuilder("[");
                      for (int n = 0; n < sizes[scenario]; n++) {
                          seed = (seed * 6364136223846793005L + 1442695040888963407L);
                          int which = (int) Long.remainderUnsigned(seed >>> 33, items.size());
                          seed = (seed * 6364136223846793005L + 1442695040888963407L);
                          int count = 1 + (int) Long.remainderUnsigned(seed >>> 33, caps[scenario]);
                          ItemStack stack = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.parse(items.get(which))), count);
                          contents.accountStack(stack);
                          dumped.append(dumped.length() == 1 ? "" : ",").append("[\"").append(items.get(which)).append("\",")
                                .append(Math.min(count, stack.getMaxStackSize())).append(']');
                      }
                      StringBuilder ids = new StringBuilder("[");
                      for (RecipeDisplayEntry entry : entries) {
                          if (canCraft(entry, contents, context)) {
                              ids.append(ids.length() == 1 ? "" : ",").append(entry.id().index());
                          }
                      }
                      craft.add("{\"scenario\":" + scenario + ",\"stacks\":" + dumped + "],\"craftable\":" + ids + "]}");
                  }
                  write("recipe_book_craft", craft);

                  // O NBT do livro do jogador.
                  List<String> nbt = new ArrayList<>();
                  String[][] cases = {
                        {},
                        {"MEKANISM_ENRICHING:open"},
                        {"MEKANISM_ENRICHING:open", "MEKANISM_ENRICHING:filter", "MEKANISM_CRUSHING:filter", "FURNACE:open"},
                        {"MEKANISM_SMELTING:open", "MEKANISM_METALLURGIC_INFUSING:open", "MEKANISM_METALLURGIC_INFUSING:filter", "CRAFTING:filter"},
                  };
                  for (int c = 0; c < cases.length; c++) {
                      RecipeBookSettings settings = new RecipeBookSettings();
                      for (String change : cases[c]) {
                          String[] parts = change.split(":");
                          RecipeBookType type = RecipeBookType.valueOf(parts[0]);
                          if (parts[1].equals("open")) {
                              settings.setOpen(type, true);
                          } else {
                              settings.setFiltering(type, true);
                          }
                      }
                      List<ResourceKey<Recipe<?>>> known = new ArrayList<>();
                      List<ResourceKey<Recipe<?>>> highlight = new ArrayList<>();
                      for (int i = 0; i < keys.size(); i += 97 - 20 * c) {
                          known.add(keys.get(i));
                          if (i % 2 == 0) {
                              highlight.add(keys.get(i));
                          }
                      }
                      ServerRecipeBook.Packed packed = new ServerRecipeBook.Packed(settings, known, highlight);
                      Tag tag = ServerRecipeBook.Packed.CODEC.encodeStart(NbtOps.INSTANCE, packed).getOrThrow();
                      CompoundTag compound = (CompoundTag) tag;
                      StringBuilder fields = new StringBuilder("{");
                      StringBuilder lists = new StringBuilder();
                      for (String name : new TreeSet<>(compound.keySet())) {
                          Tag value = compound.get(name);
                          if (value instanceof ByteTag flag) {
                              fields.append(fields.length() == 1 ? "" : ",").append('"').append(name).append("\":").append(flag.byteValue() != 0);
                          } else if (value instanceof ListTag list) {
                              StringBuilder one = new StringBuilder("[");
                              for (Tag element : list) {
                                  one.append(one.length() == 1 ? "" : ",").append('"').append(element.asString().orElse("?")).append('"');
                              }
                              lists.append(",\"").append(name).append("\":").append(one).append(']');
                          } else {
                              fields.append(fields.length() == 1 ? "" : ",").append('"').append(name).append("\":\"").append(value).append('"');
                          }
                      }
                      ServerRecipeBook.Packed back = ServerRecipeBook.Packed.CODEC.parse(NbtOps.INSTANCE, tag).getOrThrow();
                      StringBuilder read = new StringBuilder("{");
                      for (String type : new String[]{"CRAFTING", "FURNACE", "MEKANISM_ENRICHING", "MEKANISM_CRUSHING", "MEKANISM_SMELTING", "MEKANISM_METALLURGIC_INFUSING"}) {
                          RecipeBookType bookType = RecipeBookType.valueOf(type);
                          read.append(read.length() == 1 ? "" : ",").append('"').append(type).append("\":[").append(back.settings().isOpen(bookType)).append(',')
                                .append(back.settings().isFiltering(bookType)).append(']');
                      }
                      nbt.add("{\"case\":" + c + ",\"changes\":\"" + String.join(" ", cases[c]) + "\",\"fields\":" + fields + "}" + lists + ",\"read\":" + read + "}}");
                  }
                  StringBuilder modded = new StringBuilder("[");
                  for (RecipeBookType type : RecipeBookType.values()) {
                      modded.append(modded.length() == 1 ? "" : ",").append('"').append(type.name()).append('"');
                  }
                  nbt.add("{\"types\":" + modded + "]}");
                  write("recipe_book_nbt", nbt);
              })
              .thenSucceed()
        );
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
