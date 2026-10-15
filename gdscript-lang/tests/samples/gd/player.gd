@tool
class_name Player
extends CharacterBody2D
## The player: moves and jumps.

signal hit(damage: int)

enum State { IDLE, RUN }

const SPEED := 300.0
@export var max_health: int = 3
@onready var sprite: Sprite2D = $Sprite2D

var state := State.IDLE

#region Movement
func _physics_process(delta: float) -> void:
	var direction := Input.get_axis(&"move_left", &"move_right")
	velocity.x = direction * SPEED
	if not is_on_floor():
		velocity += get_gravity() * delta
	move_and_slide()
#endregion

func _init() -> void:
	pass

func take_damage(amount: int) -> void:
	max_health -= amount
	hit.emit(amount)
	match state:
		State.IDLE:
			print("ouch\n")
		_:
			pass
	await get_tree().create_timer(0.5).timeout
	var scene: PackedScene = preload("res://scenes/explosion.tscn")
	add_child(scene.instantiate())
	super.take_damage(amount)
	print(self, null, true)
	var path := ^"Sprite2D:modulate"

func extras(node: Node, values: Array[int]) -> void:
	var square := func double(x: int) -> int: return x * 2
	if node is Sprite2D:
		var sprite_2d := node as Sprite2D
	var idle := State.IDLE
	var zero := Vector2.ZERO
	"""This block is a comment."""
