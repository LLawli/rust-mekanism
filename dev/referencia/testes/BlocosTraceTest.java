package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;
import java.util.Collection;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.block.attribute.AttributeStateFacing;
import mekanism.common.block.attribute.Attributes.AttributeComparator;
import mekanism.common.block.prefab.BlockBase;
import mekanism.common.block.states.BlockStateHelper;
import mekanism.common.block.transmitter.BlockTransmitter;
import mekanism.common.content.blocktype.BlockType;
import mekanism.common.content.blocktype.Machine;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava `blocos.jsonl`: uma linha por bloco do Mekanism (na ordem do registro de `MekanismBlocks`) com o que o original dá ao bloco e ao item dele, para
/// `tests/blocos_*.rs` conferirem o que o port declara. Por bloco: a classe, os atributos do `BlockType`, a dureza, a resistência, o som, a cor, a luz, a
/// oclusão, a reação ao pistão, o instrumento, as formas de seleção e de colisão de cada estado (`VoxelShape.toAabbs`), as propriedades de estado, as tags
/// de bloco e de item e os componentes do item. `placement`: o `facing` que `BlockStateHelper.getStateForPlacement` dá a um jogador a cada ângulo, a cada
/// inclinação e a cada face clicada, e o valor de `fluid_logged` ou `waterlogged` posto em água, lava e ar.
@ForEachTest(groups = "trace")
public class BlocosTraceTest {

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 60 * SharedConstants.TICKS_PER_SECOND, batch = "trace_blocos")
    @TestHolder(description = "Records every Mekanism block: properties, states, shapes, tags, item components and the state the placement gives.")
    public static void blocos(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  FORMAS.clear();
                  COLOCACAO.clear();
                  write("blocos", dump(helper));
                  write("blocos_formas", FORMAS);
                  write("blocos_colocacao", COLOCACAO);
              })
              .thenSucceed());
    }


    /// A mesma gravação para os blocos do Mekanism Generators (`GeneratorsBlocks`, frente `generators-conteudo`): `blocos_generators.jsonl`,
    /// `blocos_generators_formas.jsonl` e `blocos_generators_colocacao.jsonl`, e os itens do mod que não são de bloco (`GeneratorsItems`) em
    /// `blocos_generators_itens.jsonl`, na ordem do registro de itens do jogo.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 60 * SharedConstants.TICKS_PER_SECOND, batch = "trace_blocos")
    @TestHolder(description = "Records every Mekanism Generators block and item: properties, states, shapes, tags, item components and the state the placement gives.")
    public static void generators(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  FORMAS.clear();
                  COLOCACAO.clear();
                  write("blocos_generators", dump(helper, GeneratorsBlocks.BLOCKS.getPrimaryEntries()));
                  write("blocos_generators_formas", FORMAS);
                  write("blocos_generators_colocacao", COLOCACAO);
                  write("blocos_generators_itens", generatorsItems());
              })
              .thenSucceed());
    }

    private static List<String> generatorsItems() {
        List<String> lines = new ArrayList<>();
        BuiltInRegistries.ITEM.stream().filter(i -> BuiltInRegistries.ITEM.getKey(i).getNamespace().equals("mekanismgenerators")).forEach(item -> {
            if (item instanceof BlockItem) {
                return;
            }
            StringBuilder out = new StringBuilder("{\"id\":").append(q(BuiltInRegistries.ITEM.getKey(item).toString()));
            out.append(",\"class\":").append(q(item.getClass().getSimpleName()));
            out.append(",\"rarity\":").append(q(String.valueOf(item.components().get(DataComponents.RARITY))));
            out.append(",\"max_stack\":").append(item.components().getOrDefault(DataComponents.MAX_STACK_SIZE, 64));
            out.append(",\"components\":").append(list(item.components().stream()
                  .map(c -> q(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type()).toString() + "=" + c.value())).sorted().collect(Collectors.toList())));
            out.append(",\"tags\":").append(list(item.builtInRegistryHolder().tags().map(t -> q(t.location().toString())).sorted().collect(Collectors.toList())));
            lines.add(out.append('}').toString());
        });
        return lines;
    }

    private static final List<String> FORMAS = new ArrayList<>();
    private static final List<String> COLOCACAO = new ArrayList<>();

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name + ".jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String q(String s) {
        return '"' + s.replace("\\", "\\\\").replace("\"", "\\\"") + '"';
    }

    private static <T extends Comparable<T>> String value(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    private static String boxes(VoxelShape shape) {
        List<String> out = new ArrayList<>();
        for (AABB box : shape.toAabbs()) {
            out.add("[" + box.minX + "," + box.minY + "," + box.minZ + "," + box.maxX + "," + box.maxY + "," + box.maxZ + "]");
        }
        return "[" + String.join(",", out) + "]";
    }

    private static String list(List<String> quoted) {
        return "[" + String.join(",", quoted) + "]";
    }

    private static List<String> dump(MekGameTestHelper helper) {
        return dump(helper, MekanismBlocks.BLOCKS.getPrimaryEntries());
    }

    private static List<String> dump(MekGameTestHelper helper, Collection<DeferredHolder<Block, ? extends Block>> entries) {
        ServerLevel level = helper.getLevel();
        BlockPos pos = helper.absolutePos(new BlockPos(1, 1, 1));
        Player player = helper.makeMockPlayerLookingAt(new BlockPos(1, 0, 1), Direction.UP);
        List<String> lines = new ArrayList<>();
        for (var entry : entries) {
            Block block = entry.get();
            lines.add(block(level, pos, player, block));
        }
        return lines;
    }

    private static String block(ServerLevel level, BlockPos pos, Player player, Block block) {
        String id = BuiltInRegistries.BLOCK.getKey(block).toString();
        BlockState base = block.defaultBlockState();
        StringBuilder out = new StringBuilder("{\"id\":").append(q(id));
        out.append(",\"class\":").append(q(block.getClass().getSimpleName()));
        BlockType type = block instanceof BlockBase<?> blockBase ? blockBase.getType() : null;
        out.append(",\"type_class\":").append(type == null ? "null" : q(type.getClass().getSimpleName()));
        out.append(",\"attributes\":").append(type == null ? "[]" : list(type.getAll().stream().map(a -> q(a.getClass().getSimpleName())).sorted().collect(Collectors.toList())));
        boolean machine = type instanceof Machine<?>;
        out.append(",\"functional_tab\":").append(block instanceof BlockTransmitter || machine);
        out.append(",\"redstone_tab\":").append(Attribute.has(block, AttributeComparator.class));
        AttributeStateFacing facing = type == null ? null : type.get(AttributeStateFacing.class);
        out.append(",\"facing_property\":").append(facing == null ? "null" : q(facing.facingProperty().getName()));
        out.append(",\"facing_placement\":").append(facing == null ? "null" : q(facing.placementType().name()));
        out.append(",\"hardness\":").append(base.getDestroySpeed(level, pos));
        out.append(",\"resistance\":").append(block.getExplosionResistance());
        //O que a explosão lê (`getExplosionResistance(state, level, pos, explosion)`): `BlockBase` troca pelo `AttributeCustomResistance`.
        out.append(",\"explosion_state\":").append(block.getExplosionResistance(base, level, pos, null));
        out.append(",\"requires_tool\":").append(base.requiresCorrectToolForDrops());
        out.append(",\"friction\":").append(block.getFriction());
        out.append(",\"has_block_entity\":").append(base.hasBlockEntity());
        out.append(",\"properties\":[");
        boolean first = true;
        for (Property<?> property : block.getStateDefinition().getProperties()) {
            out.append(first ? "" : ",").append("{\"name\":").append(q(property.getName())).append(",\"values\":[");
            boolean firstValue = true;
            for (Comparable<?> v : property.getPossibleValues()) {
                out.append(firstValue ? "" : ",").append(q(name(property, v)));
                firstValue = false;
            }
            out.append("]}");
            first = false;
        }
        out.append("],\"default\":{");
        first = true;
        for (Property<?> property : block.getStateDefinition().getProperties()) {
            out.append(first ? "" : ",").append(q(property.getName())).append(':').append(q(value(base, property)));
            first = false;
        }
        out.append("}");
        out.append(",\"tags\":").append(list(block.builtInRegistryHolder().tags().map(t -> q(t.location().toString())).sorted().collect(Collectors.toList())));
        Item item = block.asItem();
        out.append(",\"item\":{\"id\":").append(q(BuiltInRegistries.ITEM.getKey(item).toString()));
        out.append(",\"rarity\":").append(q(String.valueOf(item.components().get(DataComponents.RARITY))));
        out.append(",\"max_stack\":").append(item.components().getOrDefault(DataComponents.MAX_STACK_SIZE, 64));
        out.append(",\"components\":").append(list(item.components().stream()
              .map(c -> q(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type()).toString() + "=" + c.value())).sorted().collect(Collectors.toList())));
        out.append(",\"tags\":").append(list(item.builtInRegistryHolder().tags().map(t -> q(t.location().toString())).sorted().collect(Collectors.toList())));
        out.append("}");
        //O que o estado traz fora do `props`, `light` e `shape` (a forma é uma tabela por bloco, porque repete entre os estados que só diferem em `active` ou
        //`fluid_logged`): cada campo só aparece no estado quando difere do estado padrão, que `base` guarda.
        List<String> shapes = new ArrayList<>();
        Map<String, String> common = stateFields(level, pos, base);
        StringBuilder states = new StringBuilder();
        first = true;
        for (BlockState state : block.getStateDefinition().getPossibleStates()) {
            states.append(first ? "" : ",").append("{\"props\":{");
            boolean firstProperty = true;
            for (Property<?> property : state.getProperties()) {
                states.append(firstProperty ? "" : ",").append(q(property.getName())).append(':').append(q(value(state, property)));
                firstProperty = false;
            }
            states.append("},\"light\":").append(state.getLightEmission());
            String shape = "{\"outline\":" + boxes(state.getShape(level, pos)) + ",\"collision\":" + boxes(state.getCollisionShape(level, pos)) + "}";
            if (!shapes.contains(shape)) {
                shapes.add(shape);
            }
            states.append(",\"shape\":").append(shapes.indexOf(shape));
            for (Map.Entry<String, String> field : stateFields(level, pos, state).entrySet()) {
                if (!field.getValue().equals(common.get(field.getKey()))) {
                    states.append(',').append(q(field.getKey())).append(':').append(field.getValue());
                }
            }
            states.append('}');
            first = false;
        }
        out.append(",\"base\":{");
        first = true;
        for (Map.Entry<String, String> field : common.entrySet()) {
            out.append(first ? "" : ",").append(q(field.getKey())).append(':').append(field.getValue());
            first = false;
        }
        out.append("},\"states\":[").append(states).append("]");
        FORMAS.add("{\"id\":" + q(id) + ",\"shapes\":" + list(shapes) + "}");
        COLOCACAO.add("{\"id\":" + q(id) + ",\"placement\":" + placement(level, pos, player, block, base, facing != null) + "}");
        return out.append('}').toString();
    }

    /// Os campos de um estado que não são `props`, `light` nem `shape`, já como texto JSON.
    private static Map<String, String> stateFields(ServerLevel level, BlockPos pos, BlockState state) {
        Map<String, String> out = new LinkedHashMap<>();
        out.put("render_shape", q(state.getRenderShape().name()));
        out.put("push", q(state.getPistonPushReaction().name()));
        out.put("instrument", q(state.instrument().name()));
        out.put("map_color", String.valueOf(state.getMapColor(level, pos).id));
        out.put("sound", q(state.getSoundType().getBreakSound().location().toString()));
        out.put("can_occlude", String.valueOf(state.canOcclude()));
        out.put("solid_render", String.valueOf(state.isSolidRender()));
        out.put("dampening", String.valueOf(state.getLightDampening()));
        out.put("skylight_down", String.valueOf(state.propagatesSkylightDown()));
        out.put("random_ticks", String.valueOf(state.isRandomlyTicking()));
        out.put("signal_source", String.valueOf(state.isSignalSource()));
        out.put("analog", String.valueOf(state.hasAnalogOutputSignal()));
        out.put("fluid", q(BuiltInRegistries.FLUID.getKey(state.getFluidState().getType()).toString()));
        return out;
    }

    private static String name(Property<?> property, Comparable<?> value) {
        return nameOf(property, value);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static String nameOf(Property property, Comparable value) {
        return property.getName(value);
    }

    private static String placement(ServerLevel level, BlockPos pos, Player player, Block block, BlockState base, boolean faces) {
        StringBuilder out = new StringBuilder("{");
        boolean any = false;
        if (faces) {
            Property<?> facing = null;
            for (Property<?> property : base.getProperties()) {
                if (property.getName().equals("facing")) {
                    facing = property;
                }
            }
            //A cada ângulo (a inclinação 0, a face de cima), a cada inclinação (o ângulo 0), a cada face clicada (o ângulo 0, a inclinação 0).
            List<String> yaws = new ArrayList<>();
            for (int i = -720; i <= 720; i += 3) {
                yaws.add(q(placed(level, pos, player, block, base, facing, i, 0, Direction.UP)));
            }
            List<String> pitches = new ArrayList<>();
            for (int i = -90; i <= 90; i++) {
                pitches.add(q(placed(level, pos, player, block, base, facing, 0, i, Direction.UP)));
            }
            List<String> sides = new ArrayList<>();
            for (Direction face : Direction.values()) {
                sides.add(q(placed(level, pos, player, block, base, facing, 0, 0, face)));
            }
            out.append("\"yaw_from\":-720,\"yaw_step\":3,\"yaw\":").append(list(yaws)).append(",\"pitch_from\":-90,\"pitch\":").append(list(pitches))
                  .append(",\"face\":").append(list(sides));
            any = true;
        }
        Property<?> logged = null;
        for (Property<?> property : base.getProperties()) {
            if (property.getName().equals("fluid_logged") || property.getName().equals("waterlogged")) {
                logged = property;
            }
        }
        if (logged != null) {
            out.append(any ? "," : "").append("\"fluid\":{");
            boolean first = true;
            for (Fluid fluid : new Fluid[]{Fluids.EMPTY, Fluids.WATER, Fluids.LAVA}) {
                level.setBlock(pos, fluid.defaultFluidState().createLegacyBlock(), 3);
                BlockState state = BlockStateHelper.getStateForPlacement(base, level, pos, player, Direction.UP);
                out.append(first ? "" : ",").append(q(BuiltInRegistries.FLUID.getKey(fluid).toString())).append(':').append(q(value(state, logged)));
                first = false;
            }
            level.setBlock(pos, Blocks.AIR.defaultBlockState(), 3);
            out.append('}');
        }
        return out.append('}').toString();
    }

    private static String placed(ServerLevel level, BlockPos pos, Player player, Block block, BlockState base, Property<?> facing, float yaw, float pitch,
          Direction face) {
        player.setYRot(yaw);
        player.setXRot(pitch);
        BlockState state = BlockStateHelper.getStateForPlacement(base, level, pos, player, face);
        return facing == null ? "" : value(state, facing);
    }
}
