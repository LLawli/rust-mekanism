package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.EnumSet;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import it.unimi.dsi.fastutil.objects.Object2BooleanMaps;
import mekanism.client.render.armor.MekaSuitArmor;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.server.MinecraftServer;
import net.minecraft.core.Direction;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.EquipmentSlot;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// A armadura vestida no cliente oficial: o jogador de pé no chão, com as peças da etapa nos slots de armadura, e um print. Só age com o arquivo `cena-equip` ao lado, uma linha por etapa:
///
///   vestir nome=mekasuit pecas=head:mekasuit_helmet,chest:mekasuit_bodyarmor,legs:mekasuit_pants,feet:mekasuit_boots camera=front entidades=1 giro=0 bebe=0
///
/// `pecas` são pares `slot:item` (os ids do `mekanism:`; um id com `:` é lido inteiro). Os prints saem em `screenshots/equip_oficial_<nome>.png`. Opcionais:
/// - `camera`: `front` (padrão, `CameraType.THIRD_PERSON_FRONT`), `back` (`THIRD_PERSON_BACK`), `first` (primeira pessoa, olhando para baixo) ou `inv` (a tela do inventário de
///   sobrevivência, com o jogador no boneco);
/// - `entidades=1`: um armor stand à direita do jogador (2 blocos) e um husk (zumbi que não queima de dia, `NoAI`) à esquerda, com as mesmas peças e virados `giro` graus (0 é
///   para o sul, para a câmera da frente); `bebe=1` faz o husk um bebê;
/// - `parede=1`: um chão e uma parede de concreto preto (a parede 3 blocos atrás das figuras, vistas pela câmera), para o fundo ser o mesmo nos dois jogos.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaEquip {

    private static final Path ATIVA = Path.of("cena-equip");
    private static final double X = 300.5;
    private static final double Y = -60;
    private static final double Z = 3.5;
    private static final int ESPERA = 60;
    private static final int ENTRADA = 10;
    private static final int PAUSA = 10;

    private record Etapa(String nome, List<String[]> pecas, String camera, boolean entidades, float giro, boolean bebe, boolean parede) {
    }

    private static final List<Etapa> etapas = new ArrayList<>();
    private static boolean loaded;
    private static boolean grupos;
    private static boolean gruposFeito;
    private static boolean done;
    private static int ticks;
    private static int indice = -1;
    private static int n;
    private static int pausa;
    private static Etapa atual;

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                if (line.equals("grupos")) {
                    grupos = true;
                    continue;
                }
                String nome = "x";
                String camera = "front";
                boolean entidades = false;
                boolean bebe = false;
                boolean parede = false;
                float giro = 0;
                List<String[]> pecas = new ArrayList<>();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length < 2) {
                        continue;
                    }
                    if (kv[0].equals("nome")) {
                        nome = kv[1];
                    } else if (kv[0].equals("camera")) {
                        camera = kv[1];
                    } else if (kv[0].equals("entidades")) {
                        entidades = kv[1].equals("1");
                    } else if (kv[0].equals("bebe")) {
                        bebe = kv[1].equals("1");
                    } else if (kv[0].equals("parede")) {
                        parede = kv[1].equals("1");
                    } else if (kv[0].equals("giro")) {
                        giro = Float.parseFloat(kv[1]);
                    } else if (kv[0].equals("pecas")) {
                        for (String peca : kv[1].split(",")) {
                            pecas.add(peca.split(":", 2));
                        }
                    }
                }
                etapas.add(new Etapa(nome, pecas, camera, entidades, giro, bebe, parede));
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-equip: leitura", e);
        }
        loaded = true;
    }


    /// Os grupos de peças que `MekaSuitArmor.createQuads` monta para cada peça do traje e cada combinação de peças vestidas, sem módulos: para cada posição do corpo, quantos quads
    /// opacos, brilhantes e translúcidos (`PartData`). Grava `armor_groups.jsonl` (uma linha por peça, combinação e posição). É a referência de `tests/worn_gear.rs` para o filtro de peça,
    /// as peças `excl_` e `shared_` e a classe de cada objeto do OBJ.
    private static void gruposDoOriginal() {
        try {
            var createQuads = MekaSuitArmor.class.getDeclaredMethod("createQuads", it.unimi.dsi.fastutil.objects.Object2BooleanMap.class, Set.class, boolean.class, boolean.class);
            createQuads.setAccessible(true);
            Map<String, MekaSuitArmor> pecas = Map.of("HEAD", MekaSuitArmor.HELMET, "CHEST", MekaSuitArmor.BODYARMOR, "LEGS", MekaSuitArmor.PANTS, "FEET", MekaSuitArmor.BOOTS);
            EquipmentSlot[] slots = {EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET};
            List<String> linhas = new ArrayList<>();
            for (EquipmentSlot peca : slots) {
                for (int mascara = 0; mascara < 16; mascara++) {
                    Set<EquipmentSlot> vestidas = EnumSet.noneOf(EquipmentSlot.class);
                    for (int i = 0; i < 4; i++) {
                        if ((mascara & (1 << i)) != 0) {
                            vestidas.add(slots[i]);
                        }
                    }
                    if (!vestidas.contains(peca)) {
                        continue;
                    }
                    Map<?, ?> resultado = (Map<?, ?>) createQuads.invoke(pecas.get(peca.name()), Object2BooleanMaps.emptyMap(), vestidas, false, false);
                    for (Map.Entry<?, ?> entrada : resultado.entrySet()) {
                        Object dados = entrada.getValue();
                        int[] contagem = new int[3];
                        String[] listas = {"opaque", "emissive", "translucent"};
                        for (int k = 0; k < 3; k++) {
                            var acessor = dados.getClass().getDeclaredMethod(listas[k]);
                            acessor.setAccessible(true);
                            for (Object parte : (List<?>) acessor.invoke(dados)) {
                                // Todos os quads, os com direção de descarte também: o `automaticCulling` do OBJ dá `cull` ao quad que cai no limite do bloco, e a entidade os desenha todos.
                                var modelo = (net.minecraft.client.renderer.block.dispatch.BlockStateModelPart) parte;
                                contagem[k] += modelo.getQuads((Direction) null).size();
                                for (Direction lado : Direction.values()) {
                                    contagem[k] += modelo.getQuads(lado).size();
                                }
                            }
                        }
                        StringBuilder usadas = new StringBuilder();
                        for (EquipmentSlot v : slots) {
                            if (vestidas.contains(v)) {
                                usadas.append(usadas.length() == 0 ? "" : ",").append(v.name());
                            }
                        }
                        linhas.add(String.format(Locale.ROOT, "{\"peca\":\"%s\",\"vestidas\":[%s],\"posicao\":\"%s\",\"opacos\":%d,\"brilho\":%d,\"translucidos\":%d}", peca.name(),
                              usadas.toString().isEmpty() ? "" : "\"" + usadas.toString().replace(",", "\",\"") + "\"", entrada.getKey(), contagem[0], contagem[1], contagem[2]));
                    }
                }
            }
            Files.write(Path.of("armor_groups.jsonl"), linhas);
            Mekanism.logger.info("cena: {} linhas de grupos", linhas.size());
        } catch (ReflectiveOperationException | IOException e) {
            Mekanism.logger.error("cena-equip: grupos", e);
        }
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (done || minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (!loaded) {
            load();
        }
        ticks++;
        if (ticks == 1) {
            minecraft.options.getSoundSourceOptionInstance(SoundSource.MUSIC).set(0.0);
            minecraft.options.getSoundSourceOptionInstance(SoundSource.AMBIENT).set(0.0);
        }
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty easy"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "gamemode creative @a"));
            server.execute(() -> run(server, "fill 292 -61 -5 308 -40 12 minecraft:air"));
            server.execute(() -> run(server, "fill 292 -62 -5 308 -62 12 minecraft:grass_block"));
        }
        if (ticks < 40) {
            return;
        }
        if (pausa > 0) {
            pausa--;
            return;
        }
        if (grupos && !gruposFeito) {
            gruposFeito = true;
            gruposDoOriginal();
        }
        if (atual == null) {
            indice++;
            if (indice >= etapas.size()) {
                done = true;
                Mekanism.logger.info("cena: fim");
                minecraft.stop();
                return;
            }
            atual = etapas.get(indice);
            n = 0;
            final Etapa e = atual;
            server.execute(() -> {
                run(server, "kill @e[type=!player]");
                run(server, "clear @a");
                run(server, "gamemode " + (e.camera().equals("inv") ? "survival" : "creative") + " @a");
                run(server, String.format(Locale.ROOT, "tp @a %.2f %.2f %.2f 0 %d", X, Y, Z, e.camera().equals("first") ? 60 : 10));
                for (String[] peca : e.pecas()) {
                    run(server, "item replace entity @a armor." + peca[0] + " with mekanism:" + peca[1] + " 1");
                }
                if (e.parede()) {
                    // O fundo igual nos dois jogos: um chão e uma parede de concreto preto, a parede 3 blocos atrás das figuras vistas pela câmera.
                    int z = (int) Z + (e.camera().equals("back") ? 3 : -3);
                    // As paredes das etapas anteriores saem: a da frente ficaria entre a câmera e as figuras.
                    for (int outra : new int[] {(int) Z - 3, (int) Z + 3}) {
                        run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:air", (int) X - 10, (int) Y, outra, (int) X + 10, (int) Y + 14, outra));
                    }
                    run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:black_concrete", (int) X - 10, (int) Y - 1, (int) Z - 6, (int) X + 10, (int) Y - 1, (int) Z + 6));
                    run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:black_concrete", (int) X - 10, (int) Y, z, (int) X + 10, (int) Y + 14, z));
                }
                if (e.entidades()) {
                    run(server, String.format(Locale.ROOT, "summon minecraft:armor_stand %.2f %.2f %.2f {NoGravity:1b}", X + 2, Y, Z));
                    run(server, String.format(Locale.ROOT, "summon minecraft:husk %.2f %.2f %.2f {NoAI:1b,Silent:1b,PersistenceRequired:1b,NoGravity:1b%s}", X - 2, Y, Z,
                          e.bebe() ? ",IsBaby:1b" : ""));
                }
            });
            switch (e.camera()) {
                case "back" -> minecraft.options.setCameraType(CameraType.THIRD_PERSON_BACK);
                case "first", "inv" -> minecraft.options.setCameraType(CameraType.FIRST_PERSON);
                default -> minecraft.options.setCameraType(CameraType.THIRD_PERSON_FRONT);
            }
            return;
        }
        n++;
        if (n == ENTRADA) {
            final Etapa e = atual;
            // As peças das entidades entram depois de elas existirem: o `item replace` precisa delas no mundo.
            if (e.entidades()) {
                server.execute(() -> {
                    for (String[] peca : e.pecas()) {
                        run(server, "item replace entity @e[type=minecraft:armor_stand,limit=1] armor." + peca[0] + " with mekanism:" + peca[1] + " 1");
                        run(server, "item replace entity @e[type=minecraft:husk,limit=1] armor." + peca[0] + " with mekanism:" + peca[1] + " 1");
                    }
                    run(server, String.format(Locale.ROOT, "tp @e[type=minecraft:armor_stand,limit=1] %.2f %.2f %.2f %.1f 0", X + 2, Y, Z, e.giro()));
                    run(server, String.format(Locale.ROOT, "tp @e[type=minecraft:husk,limit=1] %.2f %.2f %.2f %.1f 0", X - 2, Y, Z, e.giro()));
                });
            }
            if (e.camera().equals("inv")) {
                minecraft.gui.setScreen(new InventoryScreen(minecraft.player));
            }
        }
        if (n == ESPERA) {
            Screenshot.grab(minecraft.gameDirectory, "equip_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            if (minecraft.gui.screen() != null) {
                minecraft.gui.setScreen(null);
            }
            atual = null;
            pausa = PAUSA;
        }
    }
}
