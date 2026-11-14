package mekanism.common.tests.network;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.BooleanSupplier;
import mekanism.common.Mekanism;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// A conexão cabo com cabo depois de sair do mundo e reabri-lo (frente `cabo-forma`, a segunda rodada): a cena de `forma.txt` (o cabo do meio com um cubo de energia a oeste e a leste, `PUSH` e
/// `PULL`, um cabo ao sul), uma reta de três cabos e uma esquina, num mundo superplano novo, salvo e reaberto no mesmo processo (como `CenaReset`). Cada `snap` escreve as linhas
/// `cena: reabrir <rotulo> <x> <y> <z> srv <mascara> raw <seis tipos> cli <mascara> <seis tipos>` de cada transmissor: a máscara `getAllCurrentConnections` e o tipo de cada lado
/// (`getConnectionTypeRaw`, e `getConnectionType` no cliente), na ordem down, up, north, south, west, east. Só age com o arquivo `cena-reabrir` ao lado.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaReabrir {

    private static final Path ATIVA = Path.of("cena-reabrir");
    private static final String MUNDO = "ReabrirA";
    private static final int OX = 0;
    private static final int OY = -61;
    private static final int OZ = 0;
    private static final String CUBO_DADOS = "{component_config:{config0:[I;4,4,4,4,4,4],eject0:0b},energy_amount:9223372036854775807L}";
    // (x, y, z) relativos à origem
    private static final int[][] CABOS = {{0, 0, 0}, {0, 0, 1}, {0, 0, 24}, {1, 0, 24}, {2, 0, 24}, {0, 0, 30}, {1, 0, 30}, {0, 0, 31}};

    private static final List<BooleanSupplier> passos = new ArrayList<>();
    private static boolean montado;
    private static boolean done;
    private static int indice;
    private static int n;
    private static int entrou;

    private static boolean noMundo(Minecraft minecraft) {
        return minecraft.level != null && minecraft.player != null && minecraft.getSingleplayerServer() != null;
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static BooleanSupplier espera(int ticks) {
        return () -> ++n >= ticks;
    }

    private static BooleanSupplier zera() {
        return () -> {
            n = 0;
            entrou = 0;
            return true;
        };
    }

    private static BooleanSupplier faz(Runnable action) {
        return () -> {
            action.run();
            return true;
        };
    }

    private static BooleanSupplier cria() {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 60) {
                minecraft.createWorldOpenFlows().createFreshLevel(MUNDO,
                      new LevelSettings(MUNDO, GameType.CREATIVE, new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true, WorldDataConfiguration.DEFAULT),
                      new WorldOptions(20261114L, false, false),
                      registries -> registries.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.FLAT).value().createWorldDimensions(),
                      new TitleScreen());
            }
            return false;
        };
    }

    private static BooleanSupplier abre() {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 20) {
                minecraft.createWorldOpenFlows().openWorld(MUNDO, () -> minecraft.setScreenAndShow(new TitleScreen()));
            }
            return false;
        };
    }

    private static BooleanSupplier depoisDeEntrar(int ticks) {
        return () -> noMundo(Minecraft.getInstance()) && ++entrou >= ticks;
    }

    private static BooleanSupplier sai() {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (minecraft.level != null) {
                minecraft.level.disconnect(Component.translatable("menu.disconnect"));
                minecraft.disconnectWithSavingScreen();
                minecraft.setScreenAndShow(new TitleScreen());
            }
            return true;
        };
    }

    private static BooleanSupplier noMenu() {
        return () -> Minecraft.getInstance().level == null && Minecraft.getInstance().getSingleplayerServer() == null;
    }

    private static void servidor(java.util.function.Consumer<MinecraftServer> action) {
        MinecraftServer server = Minecraft.getInstance().getSingleplayerServer();
        if (server != null) {
            server.execute(() -> action.accept(server));
        }
    }

    private static BlockPos pos(int x, int y, int z) {
        return new BlockPos(OX + x, OY + y, OZ + z);
    }

    private static String tipos(TileEntityTransmitter tile, boolean visivel) {
        StringBuilder sb = new StringBuilder();
        for (Direction side : Direction.values()) {
            ConnectionType type = visivel ? tile.getTransmitter().getConnectionType(side) : tile.getTransmitter().getConnectionTypeRaw(side);
            sb.append(type.name().charAt(0));
        }
        return sb.toString();
    }

    private static void snap(String rotulo) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (server == null || minecraft.level == null) {
            Mekanism.logger.info("cena: reabrir {} sem mundo", rotulo);
            return;
        }
        for (int[] c : CABOS) {
            BlockPos p = pos(c[0], c[1], c[2]);
            BlockEntity srv = server.overworld().getBlockEntity(p);
            BlockEntity cli = minecraft.level.getBlockEntity(p);
            String s = srv instanceof TileEntityTransmitter t ? String.format(Locale.ROOT, "%02x %s", t.getTransmitter().getAllCurrentConnections() & 0xff, tipos(t, false)) : "sem";
            String l = cli instanceof TileEntityTransmitter t ? String.format(Locale.ROOT, "%02x %s", t.getTransmitter().getAllCurrentConnections() & 0xff, tipos(t, true)) : "sem";
            Mekanism.logger.info("cena: reabrir {} {} {} {} srv {} cli {}", rotulo, c[0], c[1], c[2], s, l);
        }
    }

    private static void constroi(MinecraftServer server) {
        run(server, "gamerule advance_time false");
        run(server, "gamerule advance_weather false");
        run(server, "gamerule send_command_feedback false");
        run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:stone", OX - 8, OY - 1, OZ - 8, OX + 12, OY - 1, OZ + 40));
        run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:air", OX - 8, OY, OZ - 8, OX + 12, OY + 8, OZ + 40));
        int[][] blocos = {{0, 0, 0}, {0, 0, 1}, {0, 0, 24}, {1, 0, 24}, {2, 0, 24}, {0, 0, 30}, {1, 0, 30}, {0, 0, 31}};
        for (int[] b : blocos) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d mekanism:basic_universal_cable", OX + b[0], OY + b[1], OZ + b[2]));
        }
        for (int dx : new int[]{-1, 1}) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d mekanism:creative_energy_cube", OX + dx, OY, OZ));
        }
    }

    private static void dados(MinecraftServer server) {
        for (int dx : new int[]{-1, 1}) {
            run(server, String.format(Locale.ROOT, "data merge block %d %d %d %s", OX + dx, OY, OZ, CUBO_DADOS));
        }
    }

    private static void tipo(MinecraftServer server, Direction side, ConnectionType type) {
        if (server.overworld().getBlockEntity(pos(0, 0, 0)) instanceof TileEntityTransmitter tile) {
            tile.getTransmitter().setConnectionTypeRaw(side, type);
            tile.getTransmitter().onModeChange(side);
            tile.getTransmitter().refreshConnections();
            tile.getTransmitter().notifyTileChange();
            tile.sendUpdatePacket();
        }
    }

    private static void montar() {
        passos.add(cria());
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(faz(() -> servidor(CenaReabrir::constroi)));
        passos.add(zera());
        passos.add(espera(20));
        passos.add(faz(() -> servidor(CenaReabrir::dados)));
        passos.add(zera());
        passos.add(espera(40));
        passos.add(faz(() -> servidor(s -> {
            tipo(s, Direction.WEST, ConnectionType.PUSH);
            tipo(s, Direction.EAST, ConnectionType.PULL);
        })));
        passos.add(zera());
        passos.add(espera(40));
        passos.add(faz(() -> snap("antes")));
        passos.add(sai());
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(40));
        passos.add(zera());
        passos.add(abre());
        passos.add(zera());
        for (int t : new int[]{1, 10, 40, 100}) {
            final int agora = t;
            passos.add(depoisDeEntrar(agora));
            passos.add(faz(() -> snap("t" + agora)));
        }
        passos.add(faz(() -> {
            done = true;
            Mekanism.logger.info("cena: fim");
            Minecraft.getInstance().stop();
        }));
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        if (done || !Files.exists(ATIVA)) {
            return;
        }
        if (!montado) {
            montar();
            montado = true;
            minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(0.0);
        }
        if (indice < passos.size() && passos.get(indice).getAsBoolean()) {
            indice++;
        }
    }
}
