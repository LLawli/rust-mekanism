package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.server.MinecraftServer;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// Roda o roteiro `cena-blocos.json` (`dev/referencia/cenas/blocos.json`, o mesmo que o port roda por `horizon2/h2_blocos.py cena`) no cliente do original: os comandos de
/// `setup`, o F1 para esconder a tela, e para cada tiro limpa a fileira, monta os blocos por `setblock`, põe a câmera e tira o print `oficial_<tiro>.png` pelo próprio jogo.
/// Só age com o arquivo `cena-blocos.json` na pasta do jogo, para não mexer no cliente de uso normal.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaBlocos {

    private static final Path ATIVA = Path.of("cena-blocos.json");
    private static final int SETTLE = 60;
    private static final int GAP = 20;

    private record Action(int tick, Runnable run) {
    }

    private static List<Action> plan;
    private static int ticks;

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (plan == null) {
            plan = build(minecraft, server);
        }
        ticks++;
        for (Action action : plan) {
            if (action.tick == ticks) {
                action.run.run();
            }
        }
    }

    private static List<Action> build(Minecraft minecraft, MinecraftServer server) {
        JsonObject spec;
        try {
            spec = JsonParser.parseString(Files.readString(ATIVA)).getAsJsonObject();
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        List<Action> actions = new ArrayList<>();
        int at = 20;
        for (JsonElement command : spec.getAsJsonArray("setup")) {
            actions.add(new Action(at++, () -> run(server, command.getAsString())));
        }
        at += 30;
        // `"hud": true` no roteiro deixa a tela ligada (a mão com o item e a barra de atalhos aparecem; o F1 esconde a mão).
        if (!spec.has("hud") || !spec.get("hud").getAsBoolean()) {
            actions.add(new Action(at, () -> KeyMapping.click(minecraft.options.keyToggleGui.getKey())));
        }
        at += 30;
        String clear = spec.get("clear").getAsString();
        for (JsonElement element : spec.getAsJsonArray("shots")) {
            JsonObject shot = element.getAsJsonObject();
            actions.add(new Action(at, () -> run(server, clear)));
            at += 5;
            JsonArray commands = shot.getAsJsonArray("cmds");
            for (JsonElement command : commands) {
                if (command.getAsString().startsWith("#esperar ")) {
                    // `#esperar N`: N ticks do cliente sem comando (o servidor anda, e o tile publica o estado antes do `tick freeze`).
                    at += Integer.parseInt(command.getAsString().substring(9).trim());
                    continue;
                }
                actions.add(new Action(at++, () -> run(server, command.getAsString())));
            }
            actions.add(new Action(at, () -> run(server, "tp @a " + shot.get("tp").getAsString())));
            at += SETTLE;
            String name = shot.get("name").getAsString();
            actions.add(new Action(at, () -> shot(minecraft, "oficial_" + name)));
            at += GAP;
        }
        actions.add(new Action(at + 20, minecraft::stop));
        return actions;
    }

    private static void run(MinecraftServer server, String command) {
        if (command.startsWith("#cubo ")) {
            server.execute(() -> cube(server, command));
            return;
        }
        if (command.startsWith("#ejetar ")) {
            server.execute(() -> eject(server, command));
            return;
        }
        if (command.startsWith("#rede ")) {
            server.execute(() -> network(server, command));
            return;
        }
        if (command.startsWith("#conexao ")) {
            server.execute(() -> connection(server, command));
            return;
        }
        if (command.startsWith("#cor ")) {
            server.execute(() -> color(server, command));
            return;
        }
        server.execute(() -> server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command));
    }

    /// `#cubo X Y Z FRACAO [lado=tipo ...]`: a energia do Energy Cube em `x y z` como fração da capacidade (o núcleo de `RenderEnergyCube` só aparece com energia, e o alfa dele
    /// é a fração) e a configuração de energia de cada lado (`front`, `left`, `right`, `back`, `top`, `bottom` = `none`, `input` ou `output`), com o `sideChanged` que
    /// atualiza o `CubeSideState` do modelo. O port faz o mesmo por `data merge block` (`horizon2/h2_blocos.py`).
    private static void cube(MinecraftServer server, String command) {
        String[] words = command.split("\\s+");
        var pos = new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
        var tile = (mekanism.common.tile.TileEntityEnergyCube) server.overworld().getBlockEntity(pos);
        var container = (mekanism.common.capabilities.energy.BasicEnergyContainer) tile.getEnergyContainer();
        container.setEnergy((long) (Double.parseDouble(words[4]) * container.getCapacityAsLong()), null);
        var config = tile.getConfig();
        var type = config.getTransmissions().stream().filter(t -> t.getSerializedName().equals("energy")).findFirst().orElseThrow();
        for (int i = 5; i < words.length; i++) {
            String[] pair = words[i].split("=");
            var side = mekanism.api.RelativeSide.valueOf(pair[0].toUpperCase());
            var dataType = java.util.Arrays.stream(mekanism.common.tile.component.config.DataType.values()).filter(d -> d.getSerializedName().equals(pair[1])).findFirst().orElseThrow();
            config.getConfig(type).setDataType(dataType, side);
            config.sideChanged(type, side);
        }
        tile.markForSave();
    }

    /// `#cor X Y Z ORDINAL`: a cor do transportador logístico em `x y z` (`EnumColor.ordinal()`, `LogisticalTransporter.setColor` e o resto do que `onConfigure` faz). O port o põe pelo clique do
    /// Configurator agachado no centro (`onSneakRightClick`, `onConfigure`), que passa à cor seguinte (`horizon2/h2_blocos.py`).
    private static void color(MinecraftServer server, String command) {
        String[] words = command.split("\\s+");
        var pos = new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
        var tile = (mekanism.common.tile.transmitter.TileEntityTransmitter) server.overworld().getBlockEntity(pos);
        var transporter = (mekanism.common.content.network.transmitter.LogisticalTransporter) tile.getTransmitter();
        transporter.setColor(mekanism.api.text.EnumColor.BY_ID.apply(Integer.parseInt(words[4])));
        tile.markForSave();
        transporter.notifyTileChange();
        tile.sendUpdatePacket();
    }

    /// `#ejetar X Y Z`: liga o `eject` da energia do Energy Cube em `x y z` (`ConfigInfo.setEjecting`, desligado por padrão): com os lados em saída, o cubo passa a empurrar a energia para o cabo vizinho.
    /// O port faz o mesmo por `component_config:{eject0:1b}` (`horizon2/h2_blocos.py`).
    private static void eject(MinecraftServer server, String command) {
        String[] words = command.split("\\s+");
        var pos = new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
        var tile = (mekanism.common.tile.TileEntityEnergyCube) server.overworld().getBlockEntity(pos);
        var type = tile.getConfig().getTransmissions().stream().filter(t -> t.getSerializedName().equals("energy")).findFirst().orElseThrow();
        tile.getConfig().getConfig(type).setEjecting(true);
        tile.markForSave();
    }

    /// `#conexao X Y Z lado=tipo [lado=tipo ...]`: o tipo cru de cada lado do transmissor em `x y z` (`Transmitter.setConnectionTypeRaw` e o resto do que o Configurator faz a cada clique): `normal`, `push`,
    /// `pull` ou `none`. O port faz o mesmo com o clique do Configurator (`horizon2/h2_cabo_modelo.py`).
    private static void connection(MinecraftServer server, String command) {
        String[] words = command.split("\\s+");
        var pos = new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
        var tile = (mekanism.common.tile.transmitter.TileEntityTransmitter) server.overworld().getBlockEntity(pos);
        for (int i = 4; i < words.length; i++) {
            String[] pair = words[i].split("=");
            var side = net.minecraft.core.Direction.byName(pair[0]);
            var type = mekanism.common.lib.transmitter.ConnectionType.valueOf(pair[1].toUpperCase());
            // O que o Configurator faz a cada clique (`TileEntityTransmitter.onSneakRightClick`): o tipo cru, o aviso de modo, as conexões e o pacote de atualização (sem ele o cliente não sabe).
            tile.getTransmitter().setConnectionTypeRaw(side, type);
            tile.getTransmitter().onModeChange(net.minecraft.core.Direction.from3DDataValue(side.ordinal()));
            tile.getTransmitter().refreshConnections();
            tile.getTransmitter().notifyTileChange();
            tile.sendUpdatePacket();
        }
    }

    /// `#rede X Y Z fluido|quimico ID FRACAO`: põe na rede do transmissor em `x y z` o recurso `ID` com `FRACAO` da capacidade da rede, pelo contêiner dela (`DynamicBufferedResourceNetwork.getContainer().setContents`,
    /// o que um tanque que empurra faz por baixo), e escreve no log o recurso, a quantidade, a capacidade e a escala do servidor (`cena-rede`), para o port ser conferido contra o mesmo número.
    /// O port faz o mesmo por `data merge block ... {probe_push:{...}}` (`horizon2/h2_cabo_conteudo.py`).
    private static void network(MinecraftServer server, String command) {
        String[] words = command.split("\\s+");
        var pos = new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
        var tile = (mekanism.common.tile.transmitter.TileEntityTransmitter) server.overworld().getBlockEntity(pos);
        double fraction = Double.parseDouble(words[6]);
        var key = net.minecraft.resources.Identifier.parse(words[5]);
        var network = tile.getTransmitter().getTransmitterNetworkNN();
        if (words[4].equals("fluido")) {
            var fluid = (mekanism.common.content.network.FluidNetwork) network;
            var resource = net.neoforged.neoforge.transfer.fluid.FluidResource.of(net.minecraft.core.registries.BuiltInRegistries.FLUID.getValue(key));
            fluid.getContainer().setContents(resource, Math.round(fraction * fluid.getCapacity()), null);
        } else {
            var chemical = (mekanism.common.content.network.ChemicalNetwork) network;
            var resource = mekanism.api.chemical.ChemicalResource.of(server.registryAccess().getOrThrow(net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.CHEMICAL, key)));
            chemical.getContainer().setContents(resource, Math.round(fraction * chemical.getCapacity()), null);
        }
        var buffered = (mekanism.common.lib.transmitter.DynamicBufferedResourceNetwork<?, ?, ?, ?>) network;
        Mekanism.logger.info("cena-rede: {} {} {} amount {} capacity {} scale {}", words[4], words[5], words[6], buffered.getContainer().amountAsLong(), buffered.getCapacity(), buffered.currentScale);
    }

    private static void shot(Minecraft minecraft, String name) {
        // O núcleo do cubo gira com o tempo do jogo (`gameTime + partialTick`): o tempo do print vai no log, e o port o alcança com `tick step` (`h2_blocos.py`).
        Mekanism.logger.info("cena: {} gametime {}", name, minecraft.level.getGameTime());
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("cena: {}", message.getString()));
    }
}
