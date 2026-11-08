package mekanism.common.tests.network;

import com.google.gson.JsonElement;
import com.mojang.serialization.JsonOps;
import com.mojang.authlib.GameProfile;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import mekanism.common.IncompleteRecipeScanner;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.RecipeMap;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.block.AbstractFurnaceBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.OnDatapackSyncEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Rastros da frente dos restos da Fase 1 (2026-11-08).
///
/// `furnace_bio_fuel` e `blast_furnace_bio_fuel`: o `bio_fuel` (`cookingFuel(COOKING_TIME_BIO_FUEL)`, `MekanismItems.java:182`) num forno e num alto-forno,
/// tick a tick: o tempo de queima que o forno lê (`getBurnDuration`, que passa o contexto `fast_cooking` do bloco), o relógio aceso, o combustível
/// que sai do slot e o estado `lit` do bloco. O carvão vai junto como controle do vanilla.
///
/// `incomplete_recipes`: o `IncompleteRecipeScanner` (`IncompleteRecipeScanner.java`): o componente da mensagem, as receitas que a varredura acha incompletas
/// (`MekanismRecipeType.checkMyIncompleteRecipes`) e quem recebe a mensagem em cada evento (entrada de um jogador, `/reload` e `ServerStartedEvent`). Precisa
/// do data pack `mekanismtests` (`dev/referencia/testes/dados/data/mekanismtests/recipe/enriching/incomplete_*.json` e a tag vazia), que vai com a pasta
/// `data/` para `src/src/gameTest/resources/`.
@ForEachTest(groups = "trace")
public class RestosTraceTest {

    private static final BlockPos BLOCK = new BlockPos(1, 0, 0);
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    /// Ticks gravados: o forno queima 4 `bio_fuel` (400 ticks), o alto-forno 4 (200 ticks), e sobra tempo apagado.
    private static final int FURNACE_TICKS = 470;
    private static final int BLAST_TICKS = 270;

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_restos")
    @TestHolder(description = "Records a furnace burning Mekanism's bio fuel, tick by tick.")
    public static void furnaceBioFuel(final DynamicTest test) {
        fuelRun(test, Blocks.FURNACE, "furnace_bio_fuel", FURNACE_TICKS);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_restos")
    @TestHolder(description = "Records a blast furnace burning Mekanism's bio fuel, tick by tick: the fast cooking context halves the burn time.")
    public static void blastFurnaceBioFuel(final DynamicTest test) {
        fuelRun(test, Blocks.BLAST_FURNACE, "blast_furnace_bio_fuel", BLAST_TICKS);
    }

    private static void fuelRun(DynamicTest test, Block block, String name, int ticks) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, block.defaultBlockState()));
        FurnaceScene scene = new FurnaceScene(name);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(ticks, scene::write)
              .thenSucceed());
    }

    /// Um forno com 64 `raw_iron` na entrada e 4 `bio_fuel` no combustível, gravado a cada tick.
    private static final class FurnaceScene {
        private final String name;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;

        FurnaceScene(String name) {
            this.name = name;
        }

        private AbstractFurnaceBlockEntity furnace() {
            BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(BLOCK));
            return (AbstractFurnaceBlockEntity) entity;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            AbstractFurnaceBlockEntity furnace = furnace();
            // Antes de acender: o tempo de queima que o forno lê de cada combustível (`getBurnDuration`), com o contexto do bloco.
            lines.add("{\"burn\":{\"bio_fuel\":" + burnDuration(furnace, new ItemStack(MekanismItems.BIO_FUEL.asItem()))
                  + ",\"coal\":" + burnDuration(furnace, new ItemStack(Items.COAL)) + "}}");
            furnace.setItem(0, new ItemStack(Items.RAW_IRON, 64));
            furnace.setItem(1, new ItemStack(MekanismItems.BIO_FUEL.asItem(), 4));
        }

        private int burnDuration(AbstractFurnaceBlockEntity furnace, ItemStack fuel) {
            try {
                Method method = AbstractFurnaceBlockEntity.class.getDeclaredMethod("getBurnDuration", net.minecraft.server.level.ServerLevel.class, ItemStack.class);
                method.setAccessible(true);
                return (int) method.invoke(furnace, helper.getLevel(), fuel);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private int data(AbstractFurnaceBlockEntity furnace, int index) {
            try {
                Field field = AbstractFurnaceBlockEntity.class.getDeclaredField("dataAccess");
                field.setAccessible(true);
                return ((net.minecraft.world.inventory.ContainerData) field.get(furnace)).get(index);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void slot(StringBuilder line, ItemStack stack) {
            line.append('[').append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                  .append("\",").append(stack.getCount()).append(']');
        }

        void record() {
            if (helper == null) {
                return;
            }
            AbstractFurnaceBlockEntity furnace = furnace();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++)
                  .append(",\"lit_time\":").append(data(furnace, 0))
                  .append(",\"lit_total\":").append(data(furnace, 1))
                  .append(",\"cooking\":").append(data(furnace, 2))
                  .append(",\"cooking_total\":").append(data(furnace, 3))
                  .append(",\"lit\":").append(helper.getLevel().getBlockState(furnace.getBlockPos()).getValue(AbstractFurnaceBlock.LIT))
                  .append(",\"input\":");
            slot(line, furnace.getItem(0));
            line.append(",\"fuel\":");
            slot(line, furnace.getItem(1));
            line.append(",\"output\":");
            slot(line, furnace.getItem(2));
            lines.add(line.append('}').toString());
        }

        void write() {
            helper = null;
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_restos")
    @TestHolder(description = "Records the Metallurgic Infuser with iron in the input and coal in the infusion slot while the tank is full of carbon: the coal waits for room.")
    public static void infuserFullTank(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.METALLURGIC_INFUSER.defaultState()));
        FullTankScene scene = new FullTankScene();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(FULL_TANK_TICKS, scene::write)
              .thenSucceed());
    }

    /// A operação de 200 ticks gasta 10 de carbono e abre espaço: a primeira conversão do carvão sai em seguida. 260 ticks passam disso.
    private static final int FULL_TANK_TICKS = 260;

    private static final class FullTankScene {
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;

        private mekanism.common.tile.machine.TileEntityMetallurgicInfuser infuser() {
            return (mekanism.common.tile.machine.TileEntityMetallurgicInfuser) helper.getLevel().getBlockEntity(helper.absolutePos(BLOCK));
        }

        /// Ferro na entrada, carvão no slot de infusão e o tanque cheio de carbono, postos como o `data merge` do jogo os põe (sem a regra do slot).
        void start(MekGameTestHelper helper) {
            this.helper = helper;
            var infuser = infuser();
            infuser.getInventorySlots().get(0).setContents(net.neoforged.neoforge.transfer.item.ItemResource.of(Items.COAL), 8, null);
            infuser.getInventorySlots().get(1).setContents(net.neoforged.neoforge.transfer.item.ItemResource.of(Items.IRON_INGOT), 8, null);
            net.minecraft.core.Holder<mekanism.api.chemical.Chemical> carbon = helper.getLevel().registryAccess().getOrThrow(mekanism.api.chemical.ChemicalIds.CARBON);
            infuser.infusionTank.setContents(mekanism.api.chemical.ChemicalResource.of(carbon), 1_000, null);
        }

        private static void slot(StringBuilder line, mekanism.api.inventory.IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null) {
                return;
            }
            var infuser = infuser();
            var energy = (mekanism.common.capabilities.energy.MachineEnergyContainer<?>) infuser.getEnergyContainer();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++)
                  .append(",\"progress\":").append(infuser.getRecipeProgress())
                  .append(",\"tank\":").append(infuser.infusionTank.amountAsLong())
                  .append(",\"input\":[");
            slot(line, infuser.getInventorySlots().get(1));
            line.append("],\"output\":[");
            slot(line, infuser.getInventorySlots().get(2));
            line.append("],\"infusion\":[");
            slot(line, infuser.getInventorySlots().get(0));
            line.append("]}");
            lines.add(line.toString());
            // A energia fica cheia: o que se mede é o espaço do tanque.
            energy.setEnergy(energy.getCapacityAsLong(), null);
        }

        void write() {
            helper = null;
            try {
                Path file = Path.of("trace", "infuser_full_tank.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_restos")
    @TestHolder(description = "Records the incomplete recipe scanner: the message component, the incomplete recipes, and who gets the message on a join, a reload and a server start.")
    public static void incompleteRecipes(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(1, 1, 1)
              .fill(0, 0, 0, 0, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> run(helper))
              .thenSucceed());
    }

    private static final String LOG_FORMAT = "constants.mekanism.log_format";

    private static boolean found() {
        try {
            Field field = IncompleteRecipeScanner.class.getDeclaredField("foundIncompleteRecipes");
            field.setAccessible(true);
            return field.getBoolean(null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void setFound(boolean value) {
        try {
            Field field = IncompleteRecipeScanner.class.getDeclaredField("foundIncompleteRecipes");
            field.setAccessible(true);
            field.setBoolean(null, value);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /// Um jogador que só conta as mensagens do Mekanism que recebe (`sendSystemMessage`): o `ServerPlayer` verdadeiro, sem conexão, porque o jogador de teste do jogo
    /// (`makeMockServerPlayerInLevel`) faz o `placeNewPlayer` real e os outros ouvintes do evento de entrada (o `CommonPlayerTracker` do Mekanism) falham sem rede.
    private static final class CountingPlayer extends ServerPlayer {
        int messages;

        CountingPlayer(MinecraftServer server, ServerLevel level, String name) {
            super(server, level, new GameProfile(UUID.nameUUIDFromBytes(name.getBytes()), name), ClientInformation.createDefault());
        }

        @Override
        public void sendSystemMessage(Component message) {
            if (message.getContents() instanceof TranslatableContents contents && contents.getKey().equals(LOG_FORMAT)) {
                messages++;
            }
        }

        int take() {
            int count = messages;
            messages = 0;
            return count;
        }
    }

    /// `PlayerList.players`, a lista de verdade (`getPlayers()` devolve uma visão que não muda): os jogadores de teste entram nela para o `/reload`.
    @SuppressWarnings("unchecked")
    private static List<ServerPlayer> playerList(MinecraftServer server) throws ReflectiveOperationException {
        Field field = PlayerList.class.getDeclaredField("players");
        field.setAccessible(true);
        return (List<ServerPlayer>) field.get(server.getPlayerList());
    }

    private static String quote(String text) {
        return "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    /// Os ids das receitas que a varredura acha incompletas, por tipo, como `checkMyIncompleteRecipes` as conta (sem o log).
    private static List<String> incompleteIds(MinecraftServer server) throws ReflectiveOperationException {
        RecipeMap recipeMap = server.getRecipeManager().recipeMap();
        Method uncached = MekanismRecipeType.class.getDeclaredMethod("getRecipesUncached", RecipeMap.class);
        uncached.setAccessible(true);
        List<String> ids = new ArrayList<>();
        for (Holder<RecipeType<?>> entry : MekanismRecipeType.RECIPE_TYPES.getEntries()) {
            MekanismRecipeType<?, ?, ?, ?, ?> type = (MekanismRecipeType<?, ?, ?, ?, ?>) entry.value();
            @SuppressWarnings("unchecked")
            java.util.Collection<net.minecraft.world.item.crafting.RecipeHolder<?>> recipes = (java.util.Collection<net.minecraft.world.item.crafting.RecipeHolder<?>>) uncached.invoke(type, recipeMap);
            for (net.minecraft.world.item.crafting.RecipeHolder<?> holder : recipes) {
                if (holder.value().placementInfo().isImpossibleToPlace()) {
                    ids.add(holder.id().identifier().toString());
                }
            }
        }
        java.util.Collections.sort(ids);
        return ids;
    }

    private static void run(MekGameTestHelper helper) {
        List<String> lines = new ArrayList<>();
        try {
            MinecraftServer server = helper.getLevel().getServer();
            RegistryOpsHolder ops = new RegistryOpsHolder(helper);
            // A mensagem: o componente que o original manda, em JSON e como texto.
            Field messageField = IncompleteRecipeScanner.class.getDeclaredField("RECIPE_WARNING");
            messageField.setAccessible(true);
            Component message = (Component) messageField.get(null);
            JsonElement json = ComponentSerialization.CODEC.encodeStart(ops.ops, message).getOrThrow();
            lines.add("{\"case\":\"component\",\"json\":" + json + ",\"string\":" + quote(message.getString()) + "}");
            // As receitas incompletas do jogo de teste: só as do data pack `mekanismtests`.
            List<String> ids = incompleteIds(server);
            lines.add("{\"case\":\"scan\",\"incomplete\":[" + String.join(",", ids.stream().map(RestosTraceTest::quote).toList()) + "]}");
            CountingPlayer first = new CountingPlayer(server, helper.getLevel(), "first");
            CountingPlayer second = new CountingPlayer(server, helper.getLevel(), "second");
            CountingPlayer third = new CountingPlayer(server, helper.getLevel(), "third");
            // `ServerStartedEvent`: refaz a varredura (o estado do começo vem do data pack) e não manda nada a ninguém.
            boolean initial = found();
            lines.add("{\"case\":\"initial\",\"found\":" + initial + "}");
            playerList(server).add(first);
            setFound(false);
            IncompleteRecipeScanner.serverStarted(new ServerStartedEvent(server));
            lines.add("{\"case\":\"server_started\",\"found\":" + found() + ",\"messages_to_online\":" + first.take() + "}");
            // O evento de um jogador que entra (`PlayerList.placeNewPlayer`, linha 193): com a varredura positiva ele recebe a mensagem. Não é operador.
            IncompleteRecipeScanner.recipes(new OnDatapackSyncEvent(server.getPlayerList(), second));
            lines.add("{\"case\":\"join_found\",\"found\":" + found() + ",\"messages\":" + second.take() + "}");
            // Com a varredura negativa não recebe, e o evento de um jogador não varre (a varredura continua negativa).
            setFound(false);
            IncompleteRecipeScanner.recipes(new OnDatapackSyncEvent(server.getPlayerList(), third));
            lines.add("{\"case\":\"join_not_found\",\"found\":" + found() + ",\"messages\":" + third.take() + "}");
            // O `/reload` (evento sem jogador): refaz a varredura (que dá positivo) e manda a todos os jogadores do servidor, também com a varredura que estava negativa.
            playerList(server).add(second);
            playerList(server).add(third);
            setFound(false);
            IncompleteRecipeScanner.recipes(new OnDatapackSyncEvent(server.getPlayerList(), null));
            int online = server.getPlayerList().getPlayers().size();
            lines.add("{\"case\":\"reload\",\"found\":" + found() + ",\"players\":" + online + ",\"messages\":[" + first.take() + "," + second.take() + "," + third.take() + "]}");
            playerList(server).remove(first);
            playerList(server).remove(second);
            playerList(server).remove(third);
            Path file = Path.of("trace", "incomplete_recipes.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (ReflectiveOperationException | IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /// O `RegistryOps` do servidor, para o `ComponentSerialization` (o `translatable` não usa registro, mas o codec o pede).
    private static final class RegistryOpsHolder {
        final net.minecraft.resources.RegistryOps<JsonElement> ops;

        RegistryOpsHolder(MekGameTestHelper helper) {
            this.ops = helper.getLevel().registryAccess().createSerializationContext(JsonOps.INSTANCE);
        }
    }
}
