# Acme Robotics — Internal API Reference

## Overview

All internal APIs use gRPC with Protocol Buffers. REST gateways are provided
for the dashboard and external integrations. Authentication uses mTLS for
service-to-service and JWT (via Auth0) for human users.

## Navigator Service

Base URL: `navigator.internal:8080`

### PlanPath
Plans an obstacle-free path from point A to point B.

```protobuf
rpc PlanPath(PathRequest) returns (PathResponse);

message PathRequest {
  string robot_id = 1;
  Position start = 2;
  Position goal = 3;
  float max_velocity = 4;  // m/s, default 1.5
}

message PathResponse {
  repeated Waypoint waypoints = 1;
  float estimated_time_seconds = 2;
  float path_length_meters = 3;
}
```

Rate limit: 100 req/s per robot. Timeout: 2 seconds.

### GetRobotPosition
Returns the current position and heading of a robot.

```protobuf
rpc GetRobotPosition(RobotId) returns (Position);
```

Rate limit: 1000 req/s (global). Latency P99: 5ms.

## Picker Service

Base URL: `picker.internal:8081`

### ExecutePick
Commands the arm to pick an item from a shelf location.

```protobuf
rpc ExecutePick(PickRequest) returns (PickResponse);

message PickRequest {
  string robot_id = 1;
  string item_sku = 2;
  ShelfLocation location = 3;
  GripStrategy strategy = 4;  // VACUUM, PINCH, or ADAPTIVE
}

message PickResponse {
  bool success = 1;
  float duration_seconds = 2;
  string failure_reason = 3;  // empty on success
}
```

Timeout: 10 seconds. Average duration: 4.2 seconds (target: 3.5s).

### CalibrateArm
Runs the arm calibration sequence. Takes the robot out of rotation for ~5 minutes.

```protobuf
rpc CalibrateArm(RobotId) returns (CalibrationResult);
```

## Orchestrator Service

Base URL: `orchestrator.internal:8082`

### AssignTask
Assigns a fulfillment task to the optimal robot based on proximity,
battery level, and current queue depth.

```protobuf
rpc AssignTask(TaskRequest) returns (TaskAssignment);

message TaskRequest {
  string order_id = 1;
  repeated string item_skus = 2;
  string zone = 3;           // ZONE_A through ZONE_F
  Priority priority = 4;     // NORMAL, HIGH, RUSH
}

message TaskAssignment {
  string robot_id = 1;
  float estimated_completion_seconds = 2;
  int32 queue_position = 3;
}
```

### FleetStatus
Returns aggregated health metrics for the entire fleet or a single site.

```protobuf
rpc FleetStatus(FleetStatusRequest) returns (FleetStatusResponse);

message FleetStatusRequest {
  string site = 1;  // "austin", "chicago", "newark", or empty for all
}

message FleetStatusResponse {
  int32 total_robots = 1;
  int32 active_robots = 2;
  int32 charging_robots = 3;
  int32 offline_robots = 4;
  float fulfillment_rate = 5;
  float mean_pick_time_seconds = 6;
}
```

## Vision Service

Base URL: `vision.internal:8083`

### DetectObjects
Runs YOLOv8 inference on a camera frame.

```protobuf
rpc DetectObjects(Frame) returns (DetectionResult);

message DetectionResult {
  repeated Detection detections = 1;
  float inference_time_ms = 2;
}

message Detection {
  string class_name = 1;
  float confidence = 2;
  BoundingBox bbox = 3;
}
```

Runs on-device (edge TPU). Latency: 15-30ms per frame.

## API Versioning Policy

- All APIs use semantic versioning (major.minor.patch)
- Breaking changes require a major version bump and 90-day deprecation notice
- New fields can be added to protobuf messages without version bump (backwards compatible)
- Deprecated endpoints are removed after 2 major versions
- Current versions: Navigator v2.4, Picker v2.1, Orchestrator v3.0, Vision v1.8
