package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.Supplier;
import mekanism.api.inventory.IInventorySlot;
import mekanism.common.inventory.container.MekanismContainer;
import mekanism.common.inventory.container.slot.ArmorSlot;
import mekanism.common.inventory.container.type.MekanismContainerType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsContainerTypes;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.PlayLevelSoundEvent;
import net.neoforged.neoforge.event.VanillaGameEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava os cinco slots que o `.armorSideBar(-20, 11, 0)` do Heat Generator acrescenta ao menu (quatro `ArmorSlot` e um `OffhandSlot`,
/// `MekanismContainer.addArmorSlots`). O menu é aberto pela fábrica registrada do tipo (`MekanismContainerType.create`, a que o servidor
/// usa), e não por `new MekanismTileContainer<>(...)`, que sai com 40 slots e sem armadura.
/// Um arquivo por grupo: `heat_generator_armor_{layout,may_place,clicks,binding,quick_move}.jsonl`.
/// Os cenários do jogador de sobrevivência e do criativo são dois jogadores falsos que abrem cada um o seu menu.
@ForEachTest(groups = "trace")
public class HeatArmorTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    /// Ticks até o jogador falso sair do primeiro tick (`Entity.firstTick`): sem isso o `onEquipItem` não toca nada.
    private static final int SETTLE = 3;
    private static final int MENU_SLOTS = 45;
    private static final int HEAD = 40;
    private static final int OFFHAND = 44;
    private static final String[] KINDS = {"head", "chest", "legs", "feet", "offhand"};
    private static final String[] LEATHER = {"leather_helmet", "leather_chestplate", "leather_leggings", "leather_boots"};
    private static final String[] IRON = {"iron_helmet", "iron_chestplate", "iron_leggings", "iron_boots"};
    private static final List<String> MAY_PLACE_ITEMS = List.of("leather_helmet", "leather_chestplate", "leather_leggings", "leather_boots",
          "iron_helmet", "iron_chestplate", "iron_leggings", "iron_boots", "elytra", "carved_pumpkin", "skeleton_skull", "player_head",
          "turtle_helmet", "shield", "dirt", "lava_bucket", "coal", "stick");

    private static void group(DynamicTest test, String name, Consumer<Scene> body, Consumer<Scene> after) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1).fill(1, 0, 0, 1, 0, 0, GeneratorsBlocks.HEAT_GENERATOR.defaultState()));
        Scene scene = new Scene(name);
        test.eventListeners().forge().addListener((final PlayLevelSoundEvent.AtPosition event) -> scene.sound(event));
        test.eventListeners().forge().addListener((final VanillaGameEvent event) -> scene.gameEvent(event));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(SETTLE, () -> body.accept(scene))
              .thenExecuteAfter(2, () -> after.accept(scene))
              .thenExecuteAfter(1, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_armor")
    @TestHolder(description = "Records the layout of the 45 slots of the Heat Generator menu opened by the registered factory.")
    public static void heatArmorLayout(final DynamicTest test) {
        group(test, "heat_generator_armor_layout", Scene::layout, scene -> {
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_armor")
    @TestHolder(description = "Records mayPlace of the armor and offhand slots of the Heat Generator menu.")
    public static void heatArmorMayPlace(final DynamicTest test) {
        group(test, "heat_generator_armor_may_place", Scene::mayPlace, scene -> {
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_armor")
    @TestHolder(description = "Records cursor clicks, number keys and drags on the armor and offhand slots of the Heat Generator menu.")
    public static void heatArmorClicks(final DynamicTest test) {
        group(test, "heat_generator_armor_clicks", Scene::clicks, scene -> {
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_armor")
    @TestHolder(description = "Records PREVENT_ARMOR_CHANGE (curse of binding) on the armor slots, in survival and in creative.")
    public static void heatArmorBinding(final DynamicTest test) {
        group(test, "heat_generator_armor_binding", Scene::binding, scene -> {
        });
    }

    /// Lote próprio: o som que o jogador produz sozinho nos ticks seguintes não tem dono no evento (só a posição), e os jogadores falsos
    /// dos outros cenários nascem no mesmo ponto.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_armor_ticks")
    @TestHolder(description = "Records quickMoveStack and the shift-click from and to the armor and offhand slots of the Heat Generator menu.")
    public static void heatArmorQuickMove(final DynamicTest test) {
        group(test, "heat_generator_armor_quick_move", Scene::quickMoves, Scene::afterQuickMove);
    }

    private static Item item(String id) {
        Item item = BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(id));
        if (item == null || BuiltInRegistries.ITEM.getKey(item).getPath().equals("air") && !id.equals("air")) {
            throw new IllegalStateException("no item " + id);
        }
        return item;
    }

    private static ItemStack stack(String id, int count) {
        return new ItemStack(item(id), count);
    }

    private static String q(String text) {
        return "\"" + text + "\"";
    }

    private static final class Session {
        private final String name;
        private final Player player;
        private final MekanismContainer menu;

        Session(String name, Player player, MekanismContainer menu) {
            this.name = name;
            this.player = player;
            this.menu = menu;
        }
    }

    private static final class Scene {
        private final String file;
        private final List<String> lines = new ArrayList<>();
        private final List<String> sounds = new ArrayList<>();
        private final List<String> events = new ArrayList<>();
        private MekGameTestHelper helper;
        private Session survival;
        private Session creative;
        private Session current;

        Scene(String file) {
            this.file = file;
        }

        // ---------------------------------------------------------------- setup

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            survival = open("survival", GameType.SURVIVAL);
            creative = open("creative", GameType.CREATIVE);
        }

        TileEntityHeatGenerator tile() {
            return (TileEntityHeatGenerator) helper.getLevel().getBlockEntity(helper.absolutePos(new BlockPos(1, 0, 0)));
        }

        /// Abre o menu como o servidor: `MekanismContainerType.create(id, inventário, tile)`, a fábrica que o `armorSideBar` monta.
        private Session open(String name, GameType type) {
            Player player = helper.makeTickingMockServerPlayerInLevel(type);
            player.getAbilities().invulnerable = true;
            MekanismContainerType<?, ?> menuType = (MekanismContainerType<?, ?>) GeneratorsContainerTypes.HEAT_GENERATOR.get();
            AbstractContainerMenu menu = menuType.create(100, player.getInventory(), (Object) tile());
            if (!(menu instanceof MekanismContainer mekanism) || menu.slots.size() != MENU_SLOTS) {
                throw new IllegalStateException("the registered factory made " + menu + " with " + (menu == null ? -1 : menu.slots.size()) + " slots, expected " + MENU_SLOTS);
            }
            return new Session(name, player, mekanism);
        }

        void write() {
            try {
                Path path = Path.of("trace", file + ".jsonl");
                Files.createDirectories(path.getParent());
                Files.write(path, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        // ---------------------------------------------------------------- events

        void sound(PlayLevelSoundEvent.AtPosition event) {
            if (current != null && event.getSource() == SoundSource.PLAYERS && event.getPosition().distanceToSqr(current.player.position()) < 1.0E-4) {
                sounds.add(q(event.getSound().getRegisteredName()) );
            }
        }

        void gameEvent(VanillaGameEvent event) {
            if (current != null && event.getCause() == current.player) {
                events.add(q(event.getVanillaEvent().getRegisteredName()));
            }
        }

        // ---------------------------------------------------------------- json

        private static String json(ItemStack stack) {
            if (stack.isEmpty()) {
                return "[\"\",0]";
            }
            String id = BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath();
            if (EnchantmentHelper.has(stack, EnchantmentEffectComponents.PREVENT_ARMOR_CHANGE)) {
                id += "+binding";
            }
            return "[\"" + id + "\"," + stack.getCount() + "]";
        }

        private static String slotsJson(Session s) {
            StringBuilder out = new StringBuilder("[");
            for (Slot slot : s.menu.slots) {
                if (slot.hasItem()) {
                    out.append(out.length() == 1 ? "" : ",").append('[').append(slot.index).append(",\"").append(slot.getClass().getSimpleName()).append("\",")
                          .append(json(slot.getItem()).substring(1));
                }
            }
            return out.append(']').toString();
        }

        private static String equipJson(Session s) {
            Player p = s.player;
            return "{\"mainhand\":" + json(p.getItemBySlot(EquipmentSlot.MAINHAND)) + ",\"offhand\":" + json(p.getItemBySlot(EquipmentSlot.OFFHAND))
                  + ",\"head\":" + json(p.getItemBySlot(EquipmentSlot.HEAD)) + ",\"chest\":" + json(p.getItemBySlot(EquipmentSlot.CHEST))
                  + ",\"legs\":" + json(p.getItemBySlot(EquipmentSlot.LEGS)) + ",\"feet\":" + json(p.getItemBySlot(EquipmentSlot.FEET)) + "}";
        }

        private static String kind(int index) {
            return index >= HEAD && index <= OFFHAND ? KINDS[index - HEAD] : "other";
        }

        private static String join(List<String> list) {
            return "[" + String.join(",", list) + "]";
        }

        // ---------------------------------------------------------------- state

        private ItemStack cursed(String id) {
            ItemStack stack = stack(id, 1);
            Holder<Enchantment> binding = helper.getLevel().registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.BINDING_CURSE);
            stack.enchant(binding, 1);
            return stack;
        }

        /// Esvazia o inventário inteiro (com armadura e mão), o cursor e os slots do tile, e para o gerador de queimar.
        private void reset(Session s) {
            Inventory inv = s.player.getInventory();
            for (int i = 0; i < inv.getContainerSize(); i++) {
                inv.setItem(i, ItemStack.EMPTY);
            }
            s.menu.setCarried(ItemStack.EMPTY);
            for (IInventorySlot slot : tile().getInventorySlots()) {
                slot.setContents(ItemResource.EMPTY, 0, null);
            }
            tile().lavaTank.setContents(FluidResource.EMPTY, 0, null);
            tile().energyContainer().setEnergy(0, null);
            s.player.containerMenu = s.menu;
        }

        private void setLava(boolean full) {
            if (full) {
                tile().lavaTank.setContents(FluidResource.of(Fluids.LAVA), tile().lavaTank.capacityAsLong(FluidResource.of(Fluids.LAVA)), null);
            } else {
                tile().lavaTank.setContents(FluidResource.EMPTY, 0, null);
            }
        }

        /// Põe no slot do jogador que o menu mostra no índice dado, direto no inventário (sem `setByPlayer`, então sem som).
        private void put(Session s, int menuIndex, ItemStack stack) {
            Slot slot = s.menu.getSlot(menuIndex);
            ((Inventory) slot.container).setItem(slot.getSlotIndex(), stack);
        }

        private int menuIndexOfInventory(Session s, int inventoryIndex) {
            for (Slot slot : s.menu.slots) {
                if (slot.container == s.player.getInventory() && slot.getSlotIndex() == inventoryIndex) {
                    return slot.index;
                }
            }
            throw new IllegalStateException("no menu slot for inventory index " + inventoryIndex);
        }

        private void setTile(int slot, ItemStack stack) {
            tile().getInventorySlots().get(slot).setContents(stack.isEmpty() ? ItemResource.EMPTY : ItemResource.of(stack.getItem()), stack.getCount(), null);
        }

        /// Roda a ação e grava uma linha: o que havia antes, o que ficou, os sons e os eventos de jogo que ela produziu.
        private void act(Session s, String group, String name, String fields, Supplier<String> action) {
            sounds.clear();
            events.clear();
            current = s;
            String beforeSlots = slotsJson(s);
            String beforeCarried = json(s.menu.getCarried());
            String extra = action.get();
            lines.add("{\"group\":\"" + group + "\",\"case\":\"" + name + "\",\"session\":\"" + s.name + "\"" + fields + ",\"carried_before\":" + beforeCarried
                  + ",\"before\":" + beforeSlots + extra + ",\"slots\":" + slotsJson(s) + ",\"carried\":" + json(s.menu.getCarried()) + ",\"equip\":"
                  + equipJson(s) + ",\"sounds\":" + join(sounds) + ",\"events\":" + join(events) + "}");
            current = null;
        }

        // ---------------------------------------------------------------- 1. layout

        void layout() {
            Session s = survival;
            Inventory inv = s.player.getInventory();
            lines.add("{\"group\":\"summary\",\"factory\":\"MekanismContainerType.create(id, inventory, tile)\",\"menu_slots\":" + s.menu.slots.size()
                  + ",\"menu_class\":" + q(s.menu.getClass().getName()) + ",\"inventory_container_size\":" + inv.getContainerSize() + ",\"armor_slots\":"
                  + count(s, "ArmorSlot") + ",\"offhand_slots\":" + count(s, "OffhandSlot") + "}");
            for (Slot slot : s.menu.slots) {
                Identifier icon = slot.getNoItemIcon();
                String equipment = "null";
                String mapped = "null";
                if (slot instanceof ArmorSlot armor) {
                    equipment = q(armorType(armor).name());
                }
                if (slot.container == inv && Inventory.EQUIPMENT_SLOT_MAPPING.containsKey(slot.getSlotIndex())) {
                    mapped = q(Inventory.EQUIPMENT_SLOT_MAPPING.get(slot.getSlotIndex()).name());
                }
                lines.add("{\"group\":\"layout\",\"index\":" + slot.index + ",\"class\":" + q(slot.getClass().getSimpleName()) + ",\"x\":" + slot.x + ",\"y\":" + slot.y
                      + ",\"container_slot\":" + slot.getSlotIndex() + ",\"in_player_inventory\":" + (slot.container == inv) + ",\"max_stack\":" + slot.getMaxStackSize()
                      + ",\"max_stack_for_dirt\":" + slot.getMaxStackSize(stack("dirt", 64)) + ",\"max_stack_for_helmet\":" + slot.getMaxStackSize(stack("iron_helmet", 1))
                      + ",\"active\":" + slot.isActive() + ",\"no_item_icon\":" + (icon == null ? "null" : q(icon.toString())) + ",\"equipment_slot\":" + equipment
                      + ",\"inventory_equipment_mapping\":" + mapped + "}");
            }
        }

        private static long count(Session s, String simpleName) {
            return s.menu.slots.stream().filter(slot -> slot.getClass().getSimpleName().equals(simpleName)).count();
        }

        private static EquipmentSlot armorType(ArmorSlot slot) {
            try {
                Field field = ArmorSlot.class.getDeclaredField("slotType");
                field.setAccessible(true);
                return (EquipmentSlot) field.get(slot);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        // ---------------------------------------------------------------- 2. mayPlace

        void mayPlace() {
            Session s = survival;
            reset(s);
            for (int index = HEAD; index <= OFFHAND; index++) {
                for (String id : MAY_PLACE_ITEMS) {
                    for (int count : new int[]{1, 2}) {
                        if (count == 2 && !id.equals("leather_helmet") && !id.equals("dirt")) {
                            continue;
                        }
                        ItemStack probe = stack(id, count);
                        Slot slot = s.menu.getSlot(index);
                        lines.add("{\"group\":\"may_place\",\"slot\":" + index + ",\"kind\":" + q(kind(index)) + ",\"item\":" + q(id) + ",\"count\":" + count
                              + ",\"may_place\":" + slot.mayPlace(probe) + ",\"max_stack_for_item\":" + slot.getMaxStackSize(probe) + "}");
                    }
                }
            }
            // A mesma pergunta com a armadura de couro encantada com o vínculo: o encantamento não muda o mayPlace.
            for (int index = HEAD; index <= OFFHAND; index++) {
                String id = index == OFFHAND ? "leather_helmet" : LEATHER[index - HEAD];
                ItemStack probe = cursed(id);
                lines.add("{\"group\":\"may_place\",\"slot\":" + index + ",\"kind\":" + q(kind(index)) + ",\"item\":" + q(id + "+binding") + ",\"count\":1,\"may_place\":"
                      + s.menu.getSlot(index).mayPlace(probe) + "}");
            }
        }

        // ---------------------------------------------------------------- 3. clicks

        /// `how`: left, right, swap0 (tecla 1), swap40 (a tecla da mão secundária), drag (reparte), drag1 (um em cada), pickall (clique duplo).
        private void click(Session s, String name, int index, ItemStack inSlot, ItemStack carried, ItemStack hotbar0, String how) {
            reset(s);
            put(s, index, inSlot);
            s.menu.setCarried(carried);
            s.player.getInventory().setItem(0, hotbar0);
            int main9 = menuIndexOfInventory(s, 9);
            if (how.equals("pickall")) {
                // O clique duplo só recolhe quando o slot clicado está vazio: o dirt fica no 9 e o clique vai no 10.
                put(s, main9, stack("dirt", 10));
            }
            act(s, "click", name, ",\"slot\":" + index + ",\"kind\":" + q(kind(index)) + ",\"how\":" + q(how), () -> {
                switch (how) {
                    case "left" -> s.menu.clicked(index, 0, ContainerInput.PICKUP, s.player);
                    case "right" -> s.menu.clicked(index, 1, ContainerInput.PICKUP, s.player);
                    case "swap0" -> s.menu.clicked(index, 0, ContainerInput.SWAP, s.player);
                    case "swap40" -> s.menu.clicked(index, 40, ContainerInput.SWAP, s.player);
                    case "drag", "drag1" -> {
                        int type = how.equals("drag") ? 0 : 1;
                        s.menu.clicked(-999, quickCraft(0, type), ContainerInput.QUICK_CRAFT, s.player);
                        s.menu.clicked(index, quickCraft(1, type), ContainerInput.QUICK_CRAFT, s.player);
                        s.menu.clicked(main9, quickCraft(1, type), ContainerInput.QUICK_CRAFT, s.player);
                        s.menu.clicked(-999, quickCraft(2, type), ContainerInput.QUICK_CRAFT, s.player);
                    }
                    case "pickall" -> s.menu.clicked(main9 + 1, 0, ContainerInput.PICKUP_ALL, s.player);
                    default -> throw new IllegalStateException(how);
                }
                return "";
            });
        }

        /// `AbstractContainerMenu.getQuickcraftMask`: a fase (0 começa, 1 soma um slot, 2 termina) e o tipo (0 reparte, 1 põe um em cada).
        private static int quickCraft(int stage, int type) {
            return stage & 3 | (type & 3) << 2;
        }

        void clicks() {
            Session s = survival;
            ItemStack none = ItemStack.EMPTY;
            for (int k = 0; k < 4; k++) {
                int index = HEAD + k;
                String leather = LEATHER[k];
                String iron = IRON[k];
                String other = LEATHER[(k + 1) % 4];
                click(s, "place_left_matching", index, none, stack(leather, 1), none, "left");
                click(s, "place_right_matching", index, none, stack(leather, 1), none, "right");
                click(s, "place_left_matching_x2", index, none, stack(leather, 2), none, "left");
                click(s, "place_right_matching_x2", index, none, stack(leather, 2), none, "right");
                click(s, "place_left_wrong_piece", index, none, stack(other, 1), none, "left");
                click(s, "place_left_dirt", index, none, stack("dirt", 5), none, "left");
                click(s, "place_left_elytra_or_pumpkin", index, none, stack(k == 1 ? "elytra" : "carved_pumpkin", 1), none, "left");
                click(s, "take_left", index, stack(leather, 1), none, none, "left");
                click(s, "take_right", index, stack(leather, 1), none, none, "right");
                click(s, "swap_left", index, stack(leather, 1), stack(iron, 1), none, "left");
                click(s, "swap_right", index, stack(leather, 1), stack(iron, 1), none, "right");
                click(s, "swap_left_cursor_x2", index, stack(leather, 1), stack(iron, 2), none, "left");
                click(s, "same_item_left", index, stack(leather, 1), stack(leather, 1), none, "left");
                click(s, "same_item_right", index, stack(leather, 1), stack(leather, 1), none, "right");
                click(s, "occupied_cursor_dirt", index, stack(leather, 1), stack("dirt", 5), none, "left");
                click(s, "occupied_cursor_wrong_piece", index, stack(leather, 1), stack(other, 1), none, "left");
                click(s, "number_key_place", index, none, none, stack(iron, 1), "swap0");
                click(s, "number_key_swap", index, stack(leather, 1), none, stack(iron, 1), "swap0");
                click(s, "number_key_wrong_piece", index, none, none, stack(other, 1), "swap0");
                click(s, "number_key_take", index, stack(leather, 1), none, none, "swap0");
                click(s, "number_key_stack_x2", index, none, none, stack(iron, 2), "swap0");
                click(s, "number_key_swap_stack_x2", index, stack(leather, 1), none, stack(iron, 2), "swap0");
                click(s, "offhand_key_take", index, stack(leather, 1), none, none, "swap40");
                click(s, "drag_spread_x4", index, none, stack(iron, 4), none, "drag");
                click(s, "drag_one_each_x4", index, none, stack(iron, 4), none, "drag1");
            }
            int off = OFFHAND;
            click(s, "place_left_dirt_x5", off, none, stack("dirt", 5), none, "left");
            click(s, "place_right_dirt_x5", off, none, stack("dirt", 5), none, "right");
            click(s, "place_left_dirt_x64", off, none, stack("dirt", 64), none, "left");
            click(s, "place_left_helmet", off, none, stack("iron_helmet", 1), none, "left");
            click(s, "place_left_shield", off, none, stack("shield", 1), none, "left");
            click(s, "place_left_lava_bucket", off, none, stack("lava_bucket", 1), none, "left");
            click(s, "take_left", off, stack("dirt", 10), none, none, "left");
            click(s, "take_right", off, stack("dirt", 10), none, none, "right");
            click(s, "take_right_odd", off, stack("dirt", 7), none, none, "right");
            click(s, "merge_left", off, stack("dirt", 10), stack("dirt", 60), none, "left");
            click(s, "merge_right", off, stack("dirt", 10), stack("dirt", 5), none, "right");
            click(s, "merge_full_left", off, stack("dirt", 64), stack("dirt", 5), none, "left");
            click(s, "swap_left", off, stack("dirt", 10), stack("stick", 3), none, "left");
            click(s, "swap_right", off, stack("dirt", 10), stack("stick", 3), none, "right");
            click(s, "swap_left_cursor_x64", off, stack("stick", 3), stack("dirt", 64), none, "left");
            click(s, "swap_left_helmet_for_dirt", off, stack("iron_helmet", 1), stack("dirt", 5), none, "left");
            click(s, "number_key_place", off, none, none, stack("dirt", 10), "swap0");
            click(s, "number_key_swap", off, stack("dirt", 10), none, stack("stick", 3), "swap0");
            click(s, "number_key_take", off, stack("dirt", 10), none, none, "swap0");
            click(s, "number_key_helmet", off, none, none, stack("iron_helmet", 1), "swap0");
            click(s, "offhand_key_on_itself", off, stack("dirt", 10), none, none, "swap40");
            click(s, "drag_spread_x6", off, none, stack("dirt", 6), none, "drag");
            click(s, "drag_one_each_x6", off, none, stack("dirt", 6), none, "drag1");
            click(s, "pickall_with_offhand_dirt", off, stack("dirt", 20), stack("dirt", 5), none, "pickall");
        }

        // ---------------------------------------------------------------- 4. PREVENT_ARMOR_CHANGE

        void binding() {
            for (Session s : List.of(survival, creative)) {
                for (int k = 0; k < 4; k++) {
                    int index = HEAD + k;
                    String leather = LEATHER[k];
                    String iron = IRON[k];
                    ItemStack none = ItemStack.EMPTY;
                    Slot slot = s.menu.getSlot(index);
                    // mayPickup do slot, com o vínculo e sem.
                    reset(s);
                    put(s, index, cursed(leather));
                    lines.add("{\"group\":\"binding\",\"case\":\"may_pickup_cursed\",\"session\":" + q(s.name) + ",\"slot\":" + index + ",\"may_pickup\":" + slot.mayPickup(s.player)
                          + ",\"creative\":" + s.player.isCreative() + "}");
                    reset(s);
                    put(s, index, stack(leather, 1));
                    lines.add("{\"group\":\"binding\",\"case\":\"may_pickup_plain\",\"session\":" + q(s.name) + ",\"slot\":" + index + ",\"may_pickup\":" + slot.mayPickup(s.player)
                          + ",\"creative\":" + s.player.isCreative() + "}");
                    reset(s);
                    lines.add("{\"group\":\"binding\",\"case\":\"may_pickup_empty\",\"session\":" + q(s.name) + ",\"slot\":" + index + ",\"may_pickup\":" + slot.mayPickup(s.player)
                          + ",\"creative\":" + s.player.isCreative() + "}");
                    bindingClick(s, "place_cursed_into_empty", index, none, cursed(leather), none, "left");
                    bindingClick(s, "take_left", index, cursed(leather), none, none, "left");
                    bindingClick(s, "take_right", index, cursed(leather), none, none, "right");
                    bindingClick(s, "swap_left", index, cursed(leather), stack(iron, 1), none, "left");
                    bindingClick(s, "swap_right", index, cursed(leather), stack(iron, 1), none, "right");
                    bindingClick(s, "same_item_left", index, cursed(leather), stack(leather, 1), none, "left");
                    bindingClick(s, "number_key_swap", index, cursed(leather), none, stack(iron, 1), "swap0");
                    bindingClick(s, "number_key_take", index, cursed(leather), none, none, "swap0");
                    bindingClick(s, "offhand_key_take", index, cursed(leather), none, none, "swap40");
                    bindingClick(s, "shift_click_out", index, cursed(leather), none, none, "shift");
                    bindingClick(s, "quick_move_stack_out", index, cursed(leather), none, none, "direct");
                    bindingClick(s, "shift_click_out_plain", index, stack(leather, 1), none, none, "shift");
                }
            }
        }

        private void bindingClick(Session s, String name, int index, ItemStack inSlot, ItemStack carried, ItemStack hotbar0, String how) {
            if (how.equals("shift") || how.equals("direct")) {
                reset(s);
                put(s, index, inSlot);
                act(s, "binding", name, ",\"slot\":" + index + ",\"kind\":" + q(kind(index)) + ",\"how\":" + q(how), () -> {
                    if (how.equals("shift")) {
                        s.menu.clicked(index, 0, ContainerInput.QUICK_MOVE, s.player);
                        return "";
                    }
                    return ",\"returned\":" + json(s.menu.quickMoveStack(s.player, index));
                });
            } else {
                click(s, name, index, inSlot, carried, hotbar0, how);
                // `click` grava o grupo "click"; reescreve como "binding" para o arquivo ter o grupo certo.
                String last = lines.remove(lines.size() - 1);
                lines.add(last.replaceFirst("\"group\":\"click\"", "\"group\":\"binding\""));
            }
        }

        // ---------------------------------------------------------------- 5. quickMoveStack

        private static final int MAIN_SOURCE = 20;
        private static final int HOTBAR_SOURCE = 8;

        /// Enche o inventário (sem armadura nem mão) com cobblestone, que não empilha com nada do cenário, ou deixa vazio.
        private void fillInventory(Session s, String state) {
            Inventory inv = s.player.getInventory();
            switch (state) {
                case "empty" -> {
                }
                case "hotbar_full" -> {
                    for (int i = 0; i < 9; i++) {
                        inv.setItem(i, stack("cobblestone", 64));
                    }
                }
                case "full" -> {
                    for (int i = 0; i < 36; i++) {
                        inv.setItem(i, stack("cobblestone", 64));
                    }
                }
                case "partial_dirt" -> {
                    inv.setItem(0, stack("dirt", 60));
                    inv.setItem(1, stack("stick", 1));
                }
                default -> throw new IllegalStateException(state);
            }
        }

        private void fillArmor(Session s, String state) {
            if (state.equals("occupied")) {
                Inventory inv = s.player.getInventory();
                for (int k = 0; k < 4; k++) {
                    inv.setItem(39 - k, stack(LEATHER[k], 1));
                }
                inv.setItem(Inventory.SLOT_OFFHAND, stack("dirt", 3));
            }
        }

        /// Um `quickMoveStack` (`direct`, grava o devolvido) ou o shift-clique inteiro (`shift`, `clicked(QUICK_MOVE)`, que repete a chamada
        /// enquanto o slot ainda tiver o mesmo item).
        private void quickMove(Session s, String mode, String source, int inventoryIndex, ItemStack stack, String armor, String inv, String lava, String fuel) {
            reset(s);
            fillInventory(s, inv);
            fillArmor(s, armor);
            setLava(lava.equals("full"));
            if (!fuel.isEmpty()) {
                setTile(0, stack("coal", 3));
            }
            s.player.getInventory().setItem(inventoryIndex, stack.copy());
            int from = menuIndexOfInventory(s, inventoryIndex);
            act(s, "quick_move", mode + "_" + source + "_" + json(stack).replace("\"", "").replace("[", "").replace("]", "").replace(",", "x") + "_armor_" + armor + "_inv_" + inv
                  + "_lava_" + lava + (fuel.isEmpty() ? "" : "_fuel_coal3"),
                  ",\"mode\":" + q(mode) + ",\"source\":" + q(source) + ",\"from\":" + from + ",\"item\":" + json(stack) + ",\"armor\":" + q(armor) + ",\"inventory\":" + q(inv)
                        + ",\"lava\":" + q(lava) + ",\"fuel\":" + q(fuel.isEmpty() ? "empty" : "coal3"), () -> {
                      if (mode.equals("direct")) {
                          return ",\"returned\":" + json(s.menu.quickMoveStack(s.player, from));
                      }
                      s.menu.clicked(from, 0, ContainerInput.QUICK_MOVE, s.player);
                      return "";
                  });
        }

        void quickMoves() {
            Session s = survival;
            // A. do inventário principal e da barra para a armadura.
            List<ItemStack> inventoryItems = new ArrayList<>();
            for (String id : List.of("iron_helmet", "iron_chestplate", "iron_leggings", "iron_boots", "elytra", "carved_pumpkin", "skeleton_skull", "shield", "lava_bucket",
                  "coal")) {
                inventoryItems.add(stack(id, id.equals("coal") ? 8 : 1));
            }
            inventoryItems.add(stack("dirt", 10));
            inventoryItems.add(stack("leather_helmet", 2));
            inventoryItems.add(stack("diamond", 1));
            for (String mode : List.of("direct", "shift")) {
                for (String source : List.of("main", "hotbar")) {
                    int index = source.equals("main") ? MAIN_SOURCE : HOTBAR_SOURCE;
                    for (ItemStack stack : inventoryItems) {
                        for (String armor : List.of("empty", "occupied")) {
                            for (String inv : List.of("empty", "full")) {
                                if (stack.is(item("lava_bucket"))) {
                                    quickMove(s, mode, source, index, stack, armor, inv, "empty", "");
                                    quickMove(s, mode, source, index, stack, armor, inv, "full", "");
                                } else if (stack.is(item("coal"))) {
                                    quickMove(s, mode, source, index, stack, armor, inv, "empty", "");
                                    quickMove(s, mode, source, index, stack, armor, inv, "empty", "coal3");
                                } else {
                                    quickMove(s, mode, source, index, stack, armor, inv, "empty", "");
                                }
                            }
                        }
                    }
                }
            }
            // B. da armadura e da mão de volta para o inventário (e para o tile, que vem primeiro).
            for (String mode : List.of("direct", "shift")) {
                for (int k = 0; k < 4; k++) {
                    for (String inv : List.of("empty", "hotbar_full", "partial_dirt", "full")) {
                        quickMove(s, mode, KINDS[k], 39 - k, stack(LEATHER[k], 1), "none", inv, "empty", "");
                    }
                }
                for (String inv : List.of("empty", "hotbar_full", "partial_dirt", "full")) {
                    for (ItemStack stack : List.of(stack("dirt", 10), stack("dirt", 64), stack("stick", 1), stack("shield", 1), stack("iron_helmet", 1), stack("diamond", 1))) {
                        quickMove(s, mode, "offhand", Inventory.SLOT_OFFHAND, stack, "none", inv, "empty", "");
                    }
                    quickMove(s, mode, "offhand", Inventory.SLOT_OFFHAND, stack("lava_bucket", 1), "none", inv, "empty", "");
                    quickMove(s, mode, "offhand", Inventory.SLOT_OFFHAND, stack("lava_bucket", 1), "none", inv, "full", "");
                    quickMove(s, mode, "offhand", Inventory.SLOT_OFFHAND, stack("coal", 8), "none", inv, "empty", "");
                    quickMove(s, mode, "offhand", Inventory.SLOT_OFFHAND, stack("coal", 8), "none", inv, "empty", "coal3");
                }
            }
            // Um shift-clique que põe a armadura no slot e, dois ticks depois, o que o jogador produziu sozinho (som e evento de jogo).
            reset(s);
            s.player.getInventory().setItem(MAIN_SOURCE, stack("iron_helmet", 1));
            int from = menuIndexOfInventory(s, MAIN_SOURCE);
            act(s, "quick_move_then_ticks", "shift_iron_helmet_from_main_wait_2_ticks", ",\"from\":" + from, () -> {
                s.menu.clicked(from, 0, ContainerInput.QUICK_MOVE, s.player);
                return "";
            });
            current = s;
            sounds.clear();
            events.clear();
        }

        void afterQuickMove() {
            Session s = survival;
            current = s;
            lines.add("{\"group\":\"quick_move_then_ticks\",\"case\":\"two_ticks_after\",\"session\":" + q(s.name) + ",\"slots\":" + slotsJson(s) + ",\"equip\":" + equipJson(s)
                  + ",\"sounds\":" + join(sounds) + ",\"events\":" + join(events) + "}");
            current = null;
            reset(s);
        }
    }
}
