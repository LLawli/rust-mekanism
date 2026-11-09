package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.config.MekanismConfig;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// O som da Enrichment Chamber no cliente oficial, gravado em condições controladas (frente `som-plano`, 2026-11-08): o mesmo roteiro da `CenaSomMedida`, mas num mundo NOVO
/// superplano com semente, dificuldade, hora, clima e regras fixas, a máquina posta por `fill` numa coordenada fixa com `facing` explícito, e o jogador posto por `tp` numa
/// coordenada fixa com yaw e pitch explícitos. Só age com o arquivo `cena-som-plano` ao lado. A primeira linha (`mundo`) fixa o cenário; as outras são cenários, uma por linha,
/// em pares `chave=valor` (as mesmas da `CenaSomMedida`: `nome`, `muffling`, `base`, `liga`, `desliga`, `fim`, `ev=T:chave:valor`):
///
///   mundo seed=20261108 maquina=4,-60,4 facing=south pes=4.5,-60,1.5 yaw=0 pitch=0 hora=6000
///   nome=padrao
///   nome=silencio_depois muffling=0 ev=100:muffling:1
///
/// O mundo `GameTestWorld` (o que o `runGameTestClient` abre por `--quickPlaySingleplayer`) é criado aqui quando não existe (`oficial_som_plano.sh` tira o antigo do caminho): superplano
/// (`WorldPresets.FLAT`), a semente do arquivo, sem estruturas, criativo, pacífico. Quando existe (as repetições seguintes), o jogo o reabre como está. Cada tick do cliente gera uma
/// linha `tick` com o que o tile, o som, os olhos do jogador e o OUVINTE do motor de som valem; cada som de tile que o motor começa gera uma linha `play`.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaSomPlano {

    private static final Path ATIVA = Path.of("cena-som-plano");
    private static final Path ESPERA_AUDIO = Path.of("som-audio-espera");
    private static final Path AUDIO_OK = Path.of("som-audio-ok");
    private static final Path AUDIO_MUDO = Path.of("som-audio-mudo");
    private static final Path SAIDA = Path.of("som-plano.jsonl");
    /// O nome que o `build.gradle` do `runGameTestClient` passa a `--quickPlaySingleplayer`.
    private static final String MUNDO = "GameTestWorld";
    private static final int PAUSA = 10;
    /// Ticks no título antes de criar o mundo (o quickPlay falha logo no primeiro).
    private static final int ESPERA_TITULO = 100;

    private static final class Cenario {
        String nome = "x";
        int muffling;
        double base = 1.0;
        boolean enable = true;
        /// `fill_apply` (o padrão: o `fill`, e depois `applyComponents` vazio, que é o que o item faz ao ser posto e o que a `CenaSomMedida` fazia), `fill` (só o `fill`: o contêiner de
        /// energia fica com a capacidade de nascença, 20, e a Enrichment Chamber apaga 4 ticks depois de ligar) ou `set_apply` (`setBlockAndUpdate` e `applyComponents`).
        String modo = "fill_apply";
        boolean depura;
        int liga = 20;
        int desliga = 200;
        int fim = 260;
        final List<String[]> eventos = new ArrayList<>();
    }

    private static final List<Cenario> cenarios = new ArrayList<>();
    private static long semente = 20261108L;
    private static int[] maquina = {4, -60, 4};
    private static String facing = "south";
    private static double[] pes = {4.5, -60.0, 1.5};
    private static double yaw = 0;
    private static double pitch = 0;
    private static int hora = 6000;
    private static boolean loaded;
    private static boolean done;
    private static boolean audioLiberado;
    private static boolean mundoCriado;
    private static boolean mundoLogado;
    private static int esperaAudio;
    private static int noTitulo;
    private static int ticks;
    private static int indice = -1;
    private static int n;
    private static int pausa;
    private static Cenario atual;

    private static BlockPos pos() {
        return new BlockPos(maquina[0], maquina[1], maquina[2]);
    }

    private static void escreve(String linha) {
        try {
            Files.writeString(SAIDA, linha + "\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-plano: escrita", e);
        }
    }

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                boolean mundo = line.startsWith("mundo ");
                Cenario c = new Cenario();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length < 2) {
                        continue;
                    }
                    if (mundo) {
                        switch (kv[0]) {
                            case "seed" -> semente = Long.parseLong(kv[1]);
                            case "maquina" -> {
                                double[] v = parseVec(kv[1], ",");
                                maquina = new int[]{(int) v[0], (int) v[1], (int) v[2]};
                            }
                            case "facing" -> facing = kv[1];
                            case "pes" -> pes = parseVec(kv[1], ",");
                            case "yaw" -> yaw = Double.parseDouble(kv[1]);
                            case "pitch" -> pitch = Double.parseDouble(kv[1]);
                            case "hora" -> hora = Integer.parseInt(kv[1]);
                            default -> {
                            }
                        }
                        continue;
                    }
                    switch (kv[0]) {
                        case "nome" -> c.nome = kv[1];
                        case "muffling" -> c.muffling = Integer.parseInt(kv[1]);
                        case "base" -> c.base = Double.parseDouble(kv[1]);
                        case "enable" -> c.enable = Boolean.parseBoolean(kv[1]);
                        case "modo" -> c.modo = kv[1];
                        case "depura" -> c.depura = kv[1].equals("1");
                        case "liga" -> c.liga = Integer.parseInt(kv[1]);
                        case "desliga" -> c.desliga = Integer.parseInt(kv[1]);
                        case "fim" -> c.fim = Integer.parseInt(kv[1]);
                        case "ev" -> c.eventos.add(kv[1].split(":", 3));
                        default -> {
                        }
                    }
                }
                if (!mundo) {
                    cenarios.add(c);
                }
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-plano: sem arquivo", e);
        }
        loaded = true;
        try {
            Files.deleteIfExists(SAIDA);
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-plano: limpar", e);
        }
    }

    private static double[] parseVec(String text, String sep) {
        String[] p = text.split(sep);
        return new double[]{Double.parseDouble(p[0]), Double.parseDouble(p[1]), Double.parseDouble(p[2])};
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static TileEntityMekanism tile(MinecraftServer server) {
        return (TileEntityMekanism) server.overworld().getBlockEntity(pos());
    }

    private static void setMuffling(MinecraftServer server, int count) {
        TileEntityMekanism tile = tile(server);
        Holder<mekanism.api.upgrade.Upgrade> upgrade = server.overworld().registryAccess().getOrThrow(
              net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl("muffling")));
        int have = tile.getComponent().getUpgrades(upgrade);
        if (count > have) {
            tile.getComponent().addUpgrades(server.overworld().registryAccess(), upgrade, count - have);
        } else if (count < have) {
            tile.getComponent().removeUpgrade(server.overworld().registryAccess(), upgrade, count == 0);
        }
    }

    /// Liga a máquina (carvão na entrada e energia cheia) ou a desliga (energia a zero).
    private static void energia(MinecraftServer server, boolean liga) {
        TileEntityMekanism tile = tile(server);
        BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
        if (!liga) {
            container.setEnergy(0, null);
            return;
        }
        var item = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
        tile.getInventorySlots().get(0).setContents(ItemResource.of(item), 64, null);
        container.setEnergy(container.getCapacityAsLong(), null);
    }

    /// `tp` para a coordenada e a rotação fixas dos pés do jogador.
    private static void teleport(MinecraftServer server) {
        run(server, String.format(Locale.ROOT, "tp @a %.4f %.4f %.4f %.4f %.4f", pes[0], pes[1], pes[2], yaw, pitch));
    }

    @SuppressWarnings("unchecked")
    private static SoundInstance tileSound() {
        try {
            Class<?> handler = Class.forName("mekanism.client.sound.SoundHandler");
            Field field = handler.getDeclaredField("soundMap");
            field.setAccessible(true);
            var map = (it.unimi.dsi.fastutil.longs.Long2ObjectMap<SoundInstance>) field.get(null);
            return map.get(pos().asLong());
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    private static Object field(Object target, String name) {
        try {
            Field field = target.getClass().getDeclaredField(name);
            field.setAccessible(true);
            return field.get(target);
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    private static boolean fullyMuffled(TileEntityMekanism tile) {
        try {
            Method method = TileEntityMekanism.class.getDeclaredMethod("isFullyMuffled");
            method.setAccessible(true);
            return (boolean) method.invoke(tile);
        } catch (ReflectiveOperationException e) {
            return false;
        }
    }

    private static String num(double value) {
        return String.format(Locale.ROOT, "%.4f", value);
    }

    @SubscribeEvent
    public static void onPlay(PlaySoundEvent event) {
        if (atual == null || !event.getName().startsWith("tile.")) {
            return;
        }
        SoundInstance sound = event.getOriginalSound();
        if (!sound.getIdentifier().getNamespace().startsWith("mekanism")) {
            return;
        }
        Object checkInterval = field(sound, "checkInterval");
        escreve(String.format(Locale.ROOT,
              "{\"e\":\"play\",\"cenario\":\"%s\",\"n\":%d,\"nome\":\"%s\",\"id\":\"%s\",\"categoria\":\"%s\",\"volume\":%s,\"pitch\":%s,"
              + "\"atenuacao\":\"%s\",\"laco\":%s,\"atraso\":%d,\"x\":%s,\"y\":%s,\"z\":%s,\"relativo\":%s,\"comeca_mudo\":%s,\"distancia_atenuacao\":%d,\"intervalo\":%s}",
              atual.nome, n, event.getName(), sound.getIdentifier(), sound.getSource().getName(), num(sound.getVolume()), num(sound.getPitch()),
              sound.getAttenuation(), sound.isLooping(), sound.getDelay(), num(sound.getX()), num(sound.getY()), num(sound.getZ()), sound.isRelative(),
              sound.canStartSilent(), sound.getSound() == null ? -1 : sound.getSound().getAttenuationDistance(), checkInterval));
    }

    private static void aplicaEvento(MinecraftServer server, String[] ev) {
        int quando = Integer.parseInt(ev[0]);
        if (quando != n) {
            return;
        }
        switch (ev[1]) {
            case "muffling" -> server.execute(() -> setMuffling(server, Integer.parseInt(ev[2])));
            case "base" -> MekanismConfig.client.baseSoundVolume.set(Double.parseDouble(ev[2]));
            case "enable" -> MekanismConfig.client.enableMachineSounds.set(Boolean.parseBoolean(ev[2]));
            case "tick" -> server.execute(() -> run(server, "tick " + ev[2]));
            case "energia" -> server.execute(() -> energia(server, ev[2].equals("1")));
            default -> {
            }
        }
        escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"%s\",\"valor\":\"%s\"}", atual.nome, n, ev[1], ev[2]));
    }

    /// O estado do mundo uma vez, depois de o servidor aplicar as regras: semente, gerador, dificuldade, regras, hora e clima.
    private static void logaMundo(MinecraftServer server) {
        var level = server.overworld();
        String gerador = level.getChunkSource().getGenerator().getClass().getSimpleName();
        escreve(String.format(Locale.ROOT,
              "{\"e\":\"mundo\",\"criado\":%s,\"nome\":\"%s\",\"seed\":%d,\"gerador\":\"%s\",\"dificuldade\":\"%s\",\"hora\":%d,\"raining\":%s,\"thundering\":%s,"
              + "\"advance_time\":%s,\"advance_weather\":%s,\"spawn_mobs\":%s,\"modo\":\"%s\",\"chao\":\"%s\",\"maquina\":[%d,%d,%d],\"pes\":[%s,%s,%s],\"yaw\":%s,\"pitch\":%s}",
              mundoCriado, MUNDO, level.getSeed(), gerador, server.getWorldData().getDifficulty(), level.getDefaultClockTime() % 24000L, level.isRaining(), level.isThundering(),
              server.getGameRules().get(net.minecraft.world.level.gamerules.GameRules.ADVANCE_TIME),
              server.getGameRules().get(net.minecraft.world.level.gamerules.GameRules.ADVANCE_WEATHER),
              server.getGameRules().get(net.minecraft.world.level.gamerules.GameRules.SPAWN_MOBS),
              server.getPlayerList().getPlayers().isEmpty() ? "?" : server.getPlayerList().getPlayers().get(0).gameMode.getGameModeForPlayer(),
              level.getBlockState(new BlockPos(maquina[0], maquina[1] - 1, maquina[2])).getBlock(),
              maquina[0], maquina[1], maquina[2], num(pes[0]), num(pes[1]), num(pes[2]), num(yaw), num(pitch)));
    }

    private static void snapshot(Minecraft minecraft) {
        BlockEntity be = minecraft.level.getBlockEntity(pos());
        String tileJson = "null";
        if (be instanceof TileEntityMekanism tile) {
            tileJson = String.format(Locale.ROOT, "{\"lit\":%s,\"fator\":%s,\"mudo\":%s}", tile.getActive(), num(tile.getVolumeFactor()), fullyMuffled(tile));
        }
        SoundInstance sound = tileSound();
        String soundJson = "null";
        if (sound != null) {
            soundJson = String.format(Locale.ROOT, "{\"ativo\":%s,\"parado\":%s,\"volume\":%s,\"original\":%s,\"intervalo\":%s}",
                  minecraft.getSoundManager().isActive(sound), sound instanceof net.minecraft.client.resources.sounds.TickableSoundInstance t && t.isStopped(),
                  num(sound.getVolume()), field(sound, "originalVolume"), field(sound, "checkInterval"));
        }
        var player = minecraft.player.position();
        var olhos = minecraft.player.getEyePosition();
        var ouvinte = minecraft.getSoundManager().getListenerTransform();
        var op = ouvinte.position();
        var of = ouvinte.forward();
        double dx = op.x - (maquina[0] + 0.5);
        double dy = op.y - (maquina[1] + 0.5);
        double dz = op.z - (maquina[2] + 0.5);
        String linha = String.format(Locale.ROOT,
              "{\"e\":\"tick\",\"cenario\":\"%s\",\"n\":%d,\"gt\":%d,\"normal\":%s,\"tile\":%s,\"som\":%s,\"pes\":[%s,%s,%s],\"olhos\":[%s,%s,%s],\"ouvinte\":[%s,%s,%s],\"frente\":[%s,%s,%s],"
              + "\"distancia\":%s,\"base\":%s,\"liga\":%s}",
              atual.nome, n, minecraft.level.getGameTime(), minecraft.level.tickRateManager().runsNormally(), tileJson, soundJson, num(player.x), num(player.y), num(player.z),
              num(olhos.x), num(olhos.y), num(olhos.z), num(op.x), num(op.y), num(op.z), num(of.x), num(of.y), num(of.z), num(Math.sqrt(dx * dx + dy * dy + dz * dz)),
              num(MekanismConfig.client.baseSoundVolume.get()), MekanismConfig.client.enableMachineSounds.get());
        escreve(linha);
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        if (done || !Files.exists(ATIVA)) {
            return;
        }
        if (!loaded) {
            load();
        }
        // No título: o quickPlay não achou o mundo (o script tirou o antigo do caminho), então este é criado novo, superplano e com a semente da cena.
        if (minecraft.level == null || minecraft.player == null) {
            if (!mundoCriado && ++noTitulo > ESPERA_TITULO && !minecraft.getLevelSource().levelExists(MUNDO)) {
                mundoCriado = true;
                escreve("{\"e\":\"criando\",\"nome\":\"" + MUNDO + "\",\"seed\":" + semente + "}");
                minecraft.createWorldOpenFlows().createFreshLevel(MUNDO,
                      new LevelSettings(MUNDO, GameType.CREATIVE, new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true, WorldDataConfiguration.DEFAULT),
                      new WorldOptions(semente, false, false),
                      registries -> registries.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.FLAT).value().createWorldDimensions(),
                      new TitleScreen());
            }
            return;
        }
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (server == null) {
            return;
        }
        ticks++;
        if (ticks == 1) {
            // Só a categoria `block` (o som das máquinas) fica ligada; o mestre é do script (`som-audio-ok`). `directionalAudio` e o dispositivo vêm do `options.txt`.
            for (SoundSource source : SoundSource.values()) {
                if (source != SoundSource.MASTER) {
                    minecraft.options.getSoundSourceOptionInstance(source).set(source == SoundSource.BLOCKS ? 1.0 : 0.0);
                }
            }
        }
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "gamerule advance_time false"));
            server.execute(() -> run(server, "gamerule advance_weather false"));
            server.execute(() -> run(server, "gamerule spawn_mobs false"));
            server.execute(() -> run(server, "time set " + hora));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamemode creative @a"));
            server.execute(() -> run(server, "clear @a"));
        }
        if (ticks < 40) {
            return;
        }
        if (!mundoLogado) {
            mundoLogado = true;
            server.execute(() -> logaMundo(server));
        }
        if (Files.exists(ESPERA_AUDIO)) {
            if (Files.exists(AUDIO_MUDO)) {
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(0.0);
                audioLiberado = false;
            } else if (!audioLiberado) {
                if (!Files.exists(AUDIO_OK)) {
                    if (++esperaAudio > 20 * 300) {
                        escreve("{\"e\":\"fim\",\"erro\":\"sem sink nulo\"}");
                        done = true;
                        minecraft.stop();
                    }
                    return;
                }
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(1.0);
                audioLiberado = true;
            }
            if (!audioLiberado) {
                return;
            }
        }
        if (atual == null) {
            if (pausa > 0) {
                pausa--;
                return;
            }
            indice++;
            if (indice >= cenarios.size()) {
                done = true;
                escreve("{\"e\":\"fim\"}");
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(0.0);
                minecraft.stop();
                return;
            }
            atual = cenarios.get(indice);
            n = 0;
            final Cenario c = atual;
            MekanismConfig.client.baseSoundVolume.set(c.base);
            MekanismConfig.client.enableMachineSounds.set(c.enable);
            server.execute(() -> {
                int x = maquina[0];
                int y = maquina[1];
                int z = maquina[2];
                // O ar em volta da máquina (sem tocar no chão) e a máquina por `fill`, com o `facing` explícito.
                run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:air", x - 8, y, z - 8, x + 8, y + 10, z + 8));
                if (c.modo.equals("set_apply")) {
                    server.overworld().setBlockAndUpdate(pos(), mekanism.common.registries.MekanismBlocks.ENRICHMENT_CHAMBER.defaultState()
                          .trySetValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.HORIZONTAL_FACING, net.minecraft.core.Direction.byName(facing)));
                } else {
                    run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d mekanism:enrichment_chamber[facing=%s]", x, y, z, x, y, z, facing));
                }
                TileEntityMekanism tile = tile(server);
                if (!c.modo.equals("fill")) {
                    tile.applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY, net.minecraft.core.component.DataComponentPatch.EMPTY);
                }
                if (c.muffling > 0) {
                    setMuffling(server, c.muffling);
                }
                BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
                container.setEnergy(0, null);
                teleport(server);
            });
            escreve(String.format(Locale.ROOT, "{\"e\":\"cenario\",\"nome\":\"%s\",\"maquina\":\"chamber\",\"muffling\":%d,\"base\":%s,\"enable\":%s}", c.nome, c.muffling, num(c.base), c.enable));
            return;
        }
        final Cenario c = atual;
        n++;
        for (String[] ev : c.eventos) {
            aplicaEvento(server, ev);
        }
        if (n == c.liga) {
            server.execute(() -> energia(server, true));
            escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"liga\",\"valor\":\"\"}", c.nome, n));
        }
        if (n == c.desliga) {
            server.execute(() -> energia(server, false));
            escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"desliga\",\"valor\":\"\"}", c.nome, n));
        }
        snapshot(minecraft);
        if (c.depura) {
            final int nn = n;
            server.execute(() -> {
                TileEntityMekanism t = (TileEntityMekanism) server.overworld().getBlockEntity(pos());
                if (t != null) {
                    MachineEnergyContainer<?> ec = (MachineEnergyContainer<?>) t.getEnergyContainer();
                    escreve(String.format(Locale.ROOT,
                          "{\"e\":\"depura\",\"cenario\":\"%s\",\"n\":%d,\"energia\":%d,\"capacidade\":%d,\"base\":%d,\"uso\":%d,\"entrada\":%d,\"ativo\":%s,\"canFunction\":%s}",
                          c.nome, nn, ec.getAmountAsLong(), ec.getCapacityAsLong(), ec.getBaseMaxEnergy(), ec.getEnergyPerTick(), t.getInventorySlots().get(0).amountAsLong(), t.getActive(),
                          t.canFunction()));
                } else {
                    escreve("{\"e\":\"depura\",\"cenario\":\"" + c.nome + "\",\"n\":" + nn + ",\"tile\":null}");
                }
            });
        }
        if (n >= c.fim) {
            escreve(String.format(Locale.ROOT, "{\"e\":\"cenario_fim\",\"nome\":\"%s\"}", c.nome));
            server.execute(() -> server.overworld().setBlockAndUpdate(pos(), Blocks.AIR.defaultBlockState()));
            atual = null;
            pausa = PAUSA;
        }
    }
}
