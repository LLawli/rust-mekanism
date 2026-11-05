package mekanism.common.tests.network;

import java.nio.file.Files;
import java.nio.file.Path;
import mekanism.common.Mekanism;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// Monta a cena de comparação com o port e tira os prints pelo próprio jogo, sem ninguém interagir. Só age com o arquivo
/// `cena-cabo` na pasta do jogo, para não mexer no cliente de uso normal.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaCabo {

    private static final Path ATIVA = Path.of("cena-cabo");
    private static final int Y = -60;
    private static final int FIRST_X = -3;
    private static final int LAST_X = 1;
    private static final int[] ROWS = {2, 4, 6, 8};
    private static final double[] SCALES = {1.0, 0.5, 0.2, 0.05};
    private static final long ROW_CAPACITY = 40_000;
    private static final String CAMERA_WIDE = "tp @a -0.5 -60.0 -2.0 0 25";
    private static final String CAMERA_CLOSE = "tp @a 0.5 -60.0 0.5 0 45";

    // Fileiras elevadas, na altura dos olhos, para ver um conteúdo através do outro, e um tanque de água na frente.
    private static final int OVERLAP_Y = -59;
    private static final int[] OVERLAP_ROWS = {4, 6, 8};
    private static final double[] OVERLAP_SCALES = {0.5, 1.0, 0.2};
    private static final String[] OVERLAP_COMMANDS = {
          "fill -8 -60 -1 6 -57 12 minecraft:air",
          "fill -3 -60 4 1 -60 4 minecraft:stone",
          "fill -3 -60 6 1 -60 6 minecraft:stone",
          "fill -3 -60 8 1 -60 8 minecraft:stone",
          "fill -4 -60 0 2 -60 2 minecraft:stone",
          "fill -4 -59 0 2 -59 2 minecraft:glass",
          "fill -3 -59 1 1 -59 1 minecraft:water",
    };
    private static final String CAMERA_THROUGH = "tp @a -0.5 -60.0 3.5 0 2.5";
    private static final String CAMERA_OBLIQUE = "tp @a -2.5 -60.0 3.5 -35 3";
    // Fileiras empilhadas com as janelas alinhadas: o olhar reto cruza um conteúdo atrás do outro, nos dois sentidos.
    private static final int STACK_Z = 4;
    private static final int[] STACK_ROWS = {-56, -54, -52};
    private static final double[] STACK_SCALES = {0.5, 1.0, 0.2};
    private static final String[] STACK_COMMANDS = {
          "fill -8 -60 -1 6 -57 12 minecraft:air",
          "fill -8 -59 -1 6 -48 12 minecraft:air",
          "setblock -2 -50 4 minecraft:stone",
    };
    private static final String CAMERA_UP = "tp @a -0.72 -60.0 4.5 0 -90";
    private static final String CAMERA_DOWN = "tp @a -0.72 -49.0 4.5 0 90";
    // Pelo lado oposto: uma ordem de desenho fixa só falha num dos sentidos.
    private static final String CAMERA_BACK = "tp @a -0.5 -60.0 9.5 180 2.5";
    private static final String CAMERA_WATER = "tp @a -0.5 -60.0 -2.0 0 2.5";
    // A textura do conteúdo é animada: a comparação de cor usa a média de vários quadros.
    private static final int FRAMES = 8;
    private static final int FRAME_STEP = 7;

    private static int ticks;

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        switch (++ticks) {
            case 20 -> server.execute(() -> build(server));
            case 30 -> KeyMapping.click(minecraft.options.keyToggleGui.getKey());
            case 60 -> server.execute(() -> fill(server));
            case 80 -> run(server, CAMERA_WIDE);
            case 190 -> server.execute(() -> report(server));
            case 200 -> shot(minecraft, "oficial_aberta");
            case 220 -> run(server, CAMERA_CLOSE);
            case 340 -> shot(minecraft, "oficial_perto");
            case 400 -> server.execute(() -> buildOverlap(server));
            case 440 -> server.execute(() -> fillOverlap(server));
            case 460 -> run(server, CAMERA_THROUGH);
            case 520 -> shot(minecraft, "oficial_atraves");
            case 530 -> run(server, CAMERA_OBLIQUE);
            case 590 -> shot(minecraft, "oficial_obliqua");
            case 600 -> run(server, CAMERA_WATER);
            case 660 -> shot(minecraft, "oficial_agua");
            case 670 -> run(server, CAMERA_BACK);
            case 730 -> shot(minecraft, "oficial_tras");
            case 740 -> server.execute(() -> buildStack(server));
            case 780 -> server.execute(() -> fillStack(server));
            case 790 -> run(server, CAMERA_UP);
            case 850 -> shot(minecraft, "oficial_cima");
            case 860 -> run(server, CAMERA_DOWN);
            case 920 -> shot(minecraft, "oficial_baixo");
            case 960 -> minecraft.stop();
            default -> {
                if (ticks > 340 && ticks <= 340 + FRAMES * FRAME_STEP && (ticks - 340) % FRAME_STEP == 0) {
                    shot(minecraft, "oficial_quadro_" + (ticks - 340) / FRAME_STEP);
                }
            }
        }
    }

    private static void run(MinecraftServer server, String command) {
        server.execute(() -> server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command));
    }

    private static void shot(Minecraft minecraft, String name) {
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("cena: {}", message.getString()));
    }

    private static void build(MinecraftServer server) {
        run(server, "difficulty peaceful");
        run(server, "time set 6000");
        run(server, "weather clear");
        ServerLevel level = server.overworld();
        for (int x = FIRST_X - 3; x <= LAST_X + 3; x++) {
            for (int z = 0; z <= 10; z++) {
                level.setBlockAndUpdate(new BlockPos(x, Y, z), Blocks.AIR.defaultBlockState());
            }
        }
        for (int z : ROWS) {
            for (int x = FIRST_X; x <= LAST_X; x++) {
                level.setBlockAndUpdate(new BlockPos(x, Y, z), MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
            }
        }
    }

    private static void buildOverlap(MinecraftServer server) {
        for (String command : OVERLAP_COMMANDS) {
            run(server, command);
        }
        server.execute(() -> {
            ServerLevel level = server.overworld();
            for (int z : OVERLAP_ROWS) {
                for (int x = FIRST_X; x <= LAST_X; x++) {
                    level.setBlockAndUpdate(new BlockPos(x, OVERLAP_Y, z), MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
                }
            }
        });
    }

    private static void fillOverlap(MinecraftServer server) {
        ServerLevel level = server.overworld();
        for (int row = 0; row < OVERLAP_ROWS.length; row++) {
            if (level.getBlockEntity(new BlockPos(FIRST_X, OVERLAP_Y, OVERLAP_ROWS[row])) instanceof TileEntityUniversalCable cable) {
                cable.getTransmitter().getTransmitterNetworkNN().energyContainer.setEnergy((long) (ROW_CAPACITY * OVERLAP_SCALES[row]), null);
            }
        }
    }

    private static void buildStack(MinecraftServer server) {
        for (String command : STACK_COMMANDS) {
            run(server, command);
        }
        server.execute(() -> {
            ServerLevel level = server.overworld();
            for (int y : STACK_ROWS) {
                for (int x = FIRST_X; x <= LAST_X; x++) {
                    level.setBlockAndUpdate(new BlockPos(x, y, STACK_Z), MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
                }
            }
        });
    }

    private static void fillStack(MinecraftServer server) {
        ServerLevel level = server.overworld();
        for (int row = 0; row < STACK_ROWS.length; row++) {
            if (level.getBlockEntity(new BlockPos(FIRST_X, STACK_ROWS[row], STACK_Z)) instanceof TileEntityUniversalCable cable) {
                cable.getTransmitter().getTransmitterNetworkNN().energyContainer.setEnergy((long) (ROW_CAPACITY * STACK_SCALES[row]), null);
            }
        }
    }

    private static void report(MinecraftServer server) {
        ServerLevel level = server.overworld();
        for (int z : ROWS) {
            if (level.getBlockEntity(new BlockPos(FIRST_X, Y, z)) instanceof TileEntityUniversalCable cable) {
                var network = cable.getTransmitter().getTransmitterNetworkNN();
                Mekanism.logger.info("cena: fileira {} energia {} capacidade {} escala {}", z, network.getBuffer(), network.getCapacity(), network.currentScale);
            }
        }
    }

    private static void fill(MinecraftServer server) {
        ServerLevel level = server.overworld();
        for (int row = 0; row < ROWS.length; row++) {
            if (level.getBlockEntity(new BlockPos(FIRST_X, Y, ROWS[row])) instanceof TileEntityUniversalCable cable) {
                cable.getTransmitter().getTransmitterNetworkNN().energyContainer.setEnergy((long) (ROW_CAPACITY * SCALES[row]), null);
            }
        }
    }
}
