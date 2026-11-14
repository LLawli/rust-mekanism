"""Os componentes que o original solta de cada máquina (a frente `item-conteudo`): o item que o `give` e o `item replace` dão, igual nos dois jogos."""

UP = ('mekanism:upgrades={upgrades:{"mekanism:speed":3,"mekanism:energy":2},'
      'input:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:speed"}},amount:5L},'
      'output:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:energy"}},amount:7L}}')
UP_MUFFLING = ('mekanism:upgrades={upgrades:{"mekanism:muffling":1},'
               'input:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:muffling"}},amount:3L},output:{}}')
COBBLE = '{type:{id:"minecraft:cobblestone"},amount:17L}'
IRON = '{type:{id:"minecraft:iron_ingot"},amount:9L}'
REDSTONE = '{type:{id:"minecraft:redstone"},amount:5L}'
COAL = '{type:{id:"minecraft:coal"},amount:17L}'
COMMON = 'mekanism:redstone_control="high",mekanism:energy=200L'
MACHINES = [
    ("mekanism:enrichment_chamber", (2, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:crusher", (6, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:energized_smelter", (10, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:metallurgic_infuser", (2, 101, 4),
     f'{UP},mekanism:items={{containers:[{{}},{COBBLE},{IRON},{REDSTONE}]}},mekanism:chemicals={{containers:[{{type:"mekanism:bio",amount:150L}}]}},{COMMON}', "Speed", 3),
    ("mekanismgenerators:heat_generator", (6, 101, 4),
     f'{UP_MUFFLING},mekanism:items={{containers:[{COAL},{REDSTONE}]}},mekanism:fluids={{containers:[{{type:{{id:"minecraft:lava"}},amount:1500L}}]}},'
     f'mekanism:heat={{heat:4000.0d,heat_capacity:10.0d}},mekanism:redstone_control="high",mekanism:energy=200L', "Muffling", 1),
]
