package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.RelativeSide;
import mekanism.api.energy.IEnergyContainer;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.config.MekanismConfig;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityEnergyCube;
import mekanism.common.tile.TileEntityEnergyCube.CubeSideState;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.util.EnergyUtils;
import mekanism.common.util.ItemAccessUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `cubo-energia`: grava o Energy Cube tick a tick (`energy_cube_<cenário>.jsonl`). O cubo é posto por `setBlock` e recebe o padrão do item
/// (`applyComponentsFromItemStack`: `FRONT_OUT_EJECT` na energia, `FRONT_OUT_NO_EJECT` nos itens); os vizinhos são cubos básicos com a energia toda de entrada,
/// ou Enrichment Chambers. As ações de cada tick rodam em `ServerTickEvent.Pre` e a linha sai em `ServerTickEvent.Post`, depois do tick do cubo. As marcas de cada tick
/// têm a forma `energy N`, `eject true`, `mode <lado> <tipo>`, `charge <item>:<energia>:<qtd>`, `discharge ...`, `recv <face> <energia>`, e o Rust as repete.
@ForEachTest(groups = "trace")
public class EnergyCubeTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-11";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);
    private static final int TICKS = 100;


    private static net.minecraft.world.level.block.Block cubeBlock(int tier) {
        return switch (tier) {
            case 0 -> MekanismBlocks.BASIC_ENERGY_CUBE.value();
            case 1 -> MekanismBlocks.ADVANCED_ENERGY_CUBE.value();
            case 2 -> MekanismBlocks.ELITE_ENERGY_CUBE.value();
            case 3 -> MekanismBlocks.ULTIMATE_ENERGY_CUBE.value();
            default -> MekanismBlocks.CREATIVE_ENERGY_CUBE.value();
        };
    }

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Cenários
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "A basic cube ejecting from the front, with ejecting toggled and a second side set to OUTPUT.")
    public static void ejectBasic(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("eject_basic", 0, "Basic cube, 200000 stored, front OUTPUT (the item default), ejecting on; a basic cube receiver in front (north) and one on the left (west, not an output at first). Ejecting off at 10 and on at 20; the left side goes OUTPUT at 30 (two equal receivers).");
        scene.receiver(Direction.NORTH, "cube", 0).receiver(Direction.WEST, "cube", 0);
        scene.at(0, "energy 200000").at(10, "eject false").at(20, "eject true").at(30, "mode left output").at(60, "energy 3000");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "An advanced cube splitting its output among four receivers that want different amounts.")
    public static void ejectSplit(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("eject_split", 1, "Advanced cube (16000 per tick), 500000 stored, FRONT, RIGHT, LEFT and BACK OUTPUT, four basic cube receivers that accept 100, 5000, 4000000 and 4000000 (SplitInfo with needed amounts). Receivers refilled to the same room at 40 and the stored energy lowered to 30000 at 60.");
        scene.receiver(Direction.NORTH, "cube", 4_000_000 - 100).receiver(Direction.WEST, "cube", 4_000_000 - 5000)
              .receiver(Direction.SOUTH, "cube", 0).receiver(Direction.EAST, "cube", 0);
        scene.at(0, "energy 500000").at(0, "mode right output").at(0, "mode left output").at(0, "mode back output")
              .at(40, "recv north 3999900").at(40, "recv west 3995000").at(40, "recv south 0").at(40, "recv east 0").at(60, "energy 30000");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "A basic cube splitting an amount that does not divide among three receivers.")
    public static void ejectOdd(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("eject_odd", 0, "Basic cube (4000 per tick), 100000 stored, FRONT, RIGHT and LEFT OUTPUT, three basic cube receivers with room for everything: 4000 over 3 leaves a remainder of 1 (SplitInfo.getRemainderAmount). The stored energy is 7 at tick 40 and 4001 at tick 50.");
        scene.receiver(Direction.NORTH, "cube", 0).receiver(Direction.WEST, "cube", 0).receiver(Direction.EAST, "cube", 0);
        scene.at(0, "energy 100000").at(0, "mode right output").at(0, "mode left output").at(40, "energy 7").at(50, "energy 4001");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "A basic cube ejecting into Enrichment Chambers, one almost full.")
    public static void ejectMachines(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("eject_machines", 0, "Basic cube, 100000 stored, FRONT and BACK OUTPUT, an Enrichment Chamber on each (energy sides INPUT, capacity 1000000), the north one with 999000 stored. A cube with no handler (a stone block) on the right.");
        scene.receiver(Direction.NORTH, "chamber", 999_000).receiver(Direction.SOUTH, "chamber", 0);
        scene.at(0, "energy 100000").at(0, "mode back output").at(30, "energy 2000").at(45, "recv north 0");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "The charge slot of a basic cube: an Energy Tablet, a cube item, a stack that cannot be charged.")
    public static void charge(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("charge", 0, "Basic cube with ejecting off, 3000000 stored. Charge slot: an Energy Tablet at 0 (tick 0), a basic cube item with 0 (tick 30), a full tablet (tick 50), a stick (tick 60), 2 empty basic cube items (tick 70); the cube is refilled to 3000000 at 40 and 80.");
        scene.at(0, "eject false").at(0, "energy 3000000").at(0, "charge tablet:0:1").at(30, "charge basic_cube:0:1").at(40, "energy 3000000")
              .at(50, "charge tablet:1000000:1").at(60, "charge stick:0:1").at(70, "charge basic_cube:0:2").at(80, "energy 3000000");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "The discharge slot of a basic cube: a full cube item, a tablet, redstone and a redstone block.")
    public static void discharge(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("discharge", 0, "Basic cube with ejecting off, empty. Discharge slot: a full basic cube item (tick 0), a full tablet (tick 30), 5 redstone (tick 40), 1 redstone block (tick 55), a stick (tick 70); the cube is set to 3999500 at 20 and 60, and to 100 at 38 and 54 (room for the conversions).");
        scene.at(0, "eject false").at(0, "discharge basic_cube:4000000:1").at(20, "energy 3999500").at(30, "discharge tablet:1000000:1").at(40, "discharge redstone:0:5")
              .at(38, "energy 100").at(54, "energy 100").at(55, "discharge redstone_block:0:1").at(60, "energy 3999500").at(70, "discharge stick:0:1");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "The creative cube: infinite output, the slots, and nothing taken from it.")
    public static void creative(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("creative", 4, "Creative cube with Long.MAX_VALUE, ejecting on; a basic cube receiver in front. Charge slot: a tablet at 0. Discharge slot: a basic cube item full (tick 20). Energy 0 at 50 and Long.MAX_VALUE again at 70.");
        scene.receiver(Direction.NORTH, "cube", 0);
        scene.at(0, "energy 9223372036854775807").at(0, "charge tablet:0:1").at(20, "discharge basic_cube:4000000:1").at(50, "energy 0").at(70, "energy 9223372036854775807");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "The scale of the core, the comparator level and the CubeSideState of every side as the energy and the configuration change.")
    public static void scale(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("scale", 2, "Elite cube with ejecting off. The energy goes 0, 32000000 (tick 5), 64000000 (40), 1 (70), 0 (75), 63999999 (80); the sides go INPUT_OUTPUT, INPUT, NONE... between 10 and 30.");
        scene.at(0, "eject false").at(5, "energy 32000000").at(10, "mode front input_output").at(12, "mode back input").at(14, "mode top none").at(16, "mode left output")
              .at(18, "mode right input").at(20, "mode bottom input_output").at(40, "energy 64000000").at(70, "energy 1").at(75, "energy 0").at(80, "energy 63999999");
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube")
    @TestHolder(description = "An advanced cube pushing into a basic one: what the receiver reports as the energy received.")
    public static void receive(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("receive", 0, "Basic cube, all sides INPUT, ejecting off. An advanced cube behind it (north of it, facing south, so its front touches the receiver) with 100000, ejecting at 16000 per tick. The source is the first receiver; its energy is set at 0 and 40.");
        scene.source(Direction.NORTH, 1);
        scene.at(0, "eject false").at(0, "mode front input").at(0, "mode back input").at(0, "mode left input").at(0, "mode right input").at(0, "mode top input").at(0, "mode bottom input")
              .at(0, "src 100000").at(40, "src 50000");
        run(test, scene);
    }

    private static void run(DynamicTest test, Scene scene) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> scene.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.post());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper))
              .thenExecuteAfter(TICKS + 20, scene::finish)
              .thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static Item item(String name) {
        return switch (name) {
            case "tablet" -> MekanismItems_tablet();
            case "basic_cube" -> MekanismBlocks.BASIC_ENERGY_CUBE.asItem();
            case "redstone" -> Items.REDSTONE;
            case "redstone_block" -> Items.REDSTONE_BLOCK;
            case "stick" -> Items.STICK;
            default -> throw new IllegalArgumentException(name);
        };
    }

    private static Item MekanismItems_tablet() {
        return mekanism.common.registries.MekanismItems.ENERGY_TABLET.asItem();
    }

    private static ItemStack stackWith(Item item, long energy, int count) {
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        EnergyHandler handler = ContainerType.ENERGY.getCapOrUnexposed(access);
        if (handler == null || energy == 0) {
            return new ItemStack(item, count);
        }
        IEnergyContainer container = EnergyUtils.getEnergyContainer(handler);
        container.setEnergy(energy, null);
        ItemStack stack = ItemAccessUtils.asStack(access);
        stack.setCount(count);
        return stack;
    }

    private static long energyOf(ItemStack stack) {
        return stack.isEmpty() ? 0 : ContainerType.ENERGY.getOrZero(ItemResource.of(stack));
    }

    private static String name(DataType type) {
        return type.getSerializedName();
    }

    private static final class Scene {

        final String name;
        final int tier;
        final String description;
        final Map<Integer, List<String>> script = new HashMap<>();
        final List<Direction> receiverFaces = new ArrayList<>();
        final List<String> receiverKinds = new ArrayList<>();
        final List<Long> receiverEnergy = new ArrayList<>();
        Direction sourceFace;
        int sourceTier;
        final List<String> lines = new ArrayList<>();
        final List<String> marks = new ArrayList<>();
        MekGameTestHelper helper;
        boolean armed;
        boolean running;
        boolean done;
        int tick;

        Scene(String name, int tier, String description) {
            this.name = name;
            this.tier = tier;
            this.description = description;
        }

        Scene receiver(Direction face, String kind, long energy) {
            receiverFaces.add(face);
            receiverKinds.add(kind);
            receiverEnergy.add(energy);
            return this;
        }

        Scene source(Direction face, int sourceTier) {
            this.sourceFace = face;
            this.sourceTier = sourceTier;
            return this;
        }

        Scene at(int tick, String action) {
            script.computeIfAbsent(tick, t -> new ArrayList<>()).add(action);
            return this;
        }

        TileEntityEnergyCube cube() {
            return (TileEntityEnergyCube) helper.getLevel().getBlockEntity(helper.absolutePos(P1));
        }

        TileEntityMekanism neighbour(Direction face) {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(P1.relative(face)));
        }

        static RelativeSide side(String name) {
            for (RelativeSide side : RelativeSide.VALUES) {
                if (side.getSerializedName().equals(name)) {
                    return side;
                }
            }
            throw new IllegalArgumentException(name);
        }

        static DataType type(String name) {
            for (DataType type : DataType.values()) {
                if (type.getSerializedName().equals(name)) {
                    return type;
                }
            }
            throw new IllegalArgumentException(name);
        }

        void apply(String action) {
            String[] part = action.split(" ");
            TileEntityEnergyCube cube = cube();
            switch (part[0]) {
                case "energy" -> cube.energyContainer().setEnergy(Long.parseLong(part[1]), null);
                case "eject" -> cube.getConfig().getConfig(TransmissionType.ENERGY).setEjecting(Boolean.parseBoolean(part[1]));
                case "mode" -> cube.getConfig().getConfig(TransmissionType.ENERGY).setDataType(type(part[2]), side(part[1]));
                case "charge", "discharge" -> {
                    String[] spec = part[1].split(":");
                    ItemStack stack = stackWith(item(spec[0]), Long.parseLong(spec[1]), Integer.parseInt(spec[2]));
                    int index = part[0].equals("discharge") ? 0 : 1;
                    cube.getInventorySlots().get(index).setContents(ItemResource.of(stack), stack.getCount(), null);
                }
                case "recv" -> {
                    TileEntityMekanism tile = neighbour(Direction.byName(part[1]));
                    tile.getEnergyContainer().setEnergy(Long.parseLong(part[2]), null);
                }
                case "src" -> {
                    TileEntityEnergyCube source = (TileEntityEnergyCube) neighbour(sourceFace);
                    source.energyContainer().setEnergy(Long.parseLong(part[1]), null);
                }
                default -> throw new IllegalArgumentException(action);
            }
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            helper.setBlock(P1, cubeBlock(tier).defaultBlockState());
            TileEntityEnergyCube cube = cube();
            cube.applyComponentsFromItemStack(new ItemStack(cubeBlock(tier).asItem()));
            cube.energyContainer().setEnergy(0, null);
            for (int i = 0; i < receiverFaces.size(); i++) {
                Direction face = receiverFaces.get(i);
                String kind = receiverKinds.get(i);
                BlockState state = kind.equals("chamber") ? MekanismBlocks.ENRICHMENT_CHAMBER.defaultState() : MekanismBlocks.BASIC_ENERGY_CUBE.defaultState();
                helper.setBlock(P1.relative(face), state);
                TileEntityMekanism tile = neighbour(face);
                ConfigInfo info = ((mekanism.common.tile.interfaces.ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ENERGY);
                for (RelativeSide relative : RelativeSide.VALUES) {
                    info.setDataType(DataType.INPUT, relative);
                }
                if (kind.equals("chamber")) {
                    ((mekanism.common.capabilities.energy.MachineEnergyContainer<?>) tile.getEnergyContainer()).setMaxEnergy(1_000_000);
                }
                tile.getEnergyContainer().setEnergy(receiverEnergy.get(i), null);
            }
            if (sourceFace != null) {
                helper.setBlock(P1.relative(sourceFace), cubeBlock(sourceTier).defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.FACING, sourceFace.getOpposite()));
                TileEntityEnergyCube source = (TileEntityEnergyCube) neighbour(sourceFace);
                source.applyComponentsFromItemStack(new ItemStack(cubeBlock(sourceTier).asItem()));
            }
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", "energy_cube_" + name + ".jsonl");
            meta.addProperty("scenario", description);
            meta.addProperty("tier", tier);
            meta.addProperty("capacity", cube().energyContainer().getCapacityAsLong());
            meta.addProperty("transfer_rate", cube().getTier().getTransferRate());
            meta.addProperty("tablet_capacity", MekanismConfig.gear.tabletMaxEnergy.get());
            meta.addProperty("tablet_rate", MekanismConfig.gear.tabletChargeRate.get());
            JsonArray receivers = new JsonArray();
            for (int i = 0; i < receiverFaces.size(); i++) {
                JsonObject receiver = new JsonObject();
                receiver.addProperty("face", receiverFaces.get(i).getSerializedName());
                receiver.addProperty("kind", receiverKinds.get(i));
                receiver.addProperty("energy", receiverEnergy.get(i));
                receivers.add(receiver);
            }
            meta.add("receivers", receivers);
            if (sourceFace != null) {
                meta.addProperty("source_face", sourceFace.getSerializedName());
                meta.addProperty("source_tier", sourceTier);
            }
            meta.addProperty("fields", "t; marks (the actions of the tick, run in ServerTickEvent.Pre); cube: energy, scale (getEnergyScale), comparator (getCurrentRedstoneLevel), input_rate, "
                  + "charge and discharge as [item path, count, energy of the stack], sides (CubeSideState ordinal per RelativeSide, in RelativeSide.VALUES order), ejecting; "
                  + "receivers {face: energy}; source energy under src.");
            JsonObject metaLine = new JsonObject();
            metaLine.add("meta", meta);
            lines.add(metaLine.toString());
            armed = true;
        }

        void pre() {
            if (!armed || done) {
                return;
            }
            running = true;
            List<String> list = script.get(tick);
            if (list != null) {
                for (String action : list) {
                    marks.add(action);
                    apply(action);
                }
            }
        }

        void post() {
            if (!running || done) {
                return;
            }
            record();
            marks.clear();
            tick++;
            if (tick >= TICKS) {
                done = true;
            }
        }

        JsonArray slot(int index) {
            ItemStack stack = cube().getInventorySlots().get(index).asItemStack();
            JsonArray array = new JsonArray();
            array.add(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath());
            array.add(stack.getCount());
            array.add(energyOf(stack));
            return array;
        }

        void record() {
            TileEntityEnergyCube cube = cube();
            JsonObject line = new JsonObject();
            line.addProperty("t", tick);
            JsonArray markArray = new JsonArray();
            marks.forEach(markArray::add);
            line.add("marks", markArray);
            JsonObject out = new JsonObject();
            out.addProperty("energy", cube.energyContainer().getAmountAsLong());
            out.addProperty("scale", cube.getEnergyScale());
            out.addProperty("comparator", cube.getCurrentRedstoneLevel());
            out.addProperty("input_rate", cube.getInputRate());
            out.add("charge", slot(1));
            out.add("discharge", slot(0));
            CubeSideState[] states = cube.getModelData().get(TileEntityEnergyCube.SIDE_STATE_PROPERTY);
            JsonArray sides = new JsonArray();
            for (CubeSideState state : states) {
                sides.add(state.ordinal());
            }
            out.add("sides", sides);
            out.addProperty("ejecting", cube.getConfig().getConfig(TransmissionType.ENERGY).isEjecting());
            line.add("cube", out);
            JsonObject receivers = new JsonObject();
            for (Direction face : receiverFaces) {
                receivers.addProperty(face.getSerializedName(), neighbour(face).getEnergyContainer().getAmountAsLong());
            }
            line.add("receivers", receivers);
            if (sourceFace != null) {
                line.addProperty("src", ((TileEntityEnergyCube) neighbour(sourceFace)).energyContainer().getAmountAsLong());
            }
            lines.add(line.toString());
        }

        void finish() {
            try {
                Path file = Path.of("trace", "energy_cube_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
