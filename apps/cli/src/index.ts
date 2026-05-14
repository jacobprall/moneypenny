#!/usr/bin/env bun
import { Command } from "commander";

import {
  agentsCliCommand,
  chatCommand,
  cloudCommand,
  configCommand,
  doctorCommand,
  eventsCommand,
  indexCommand,
  inspectCommand,
  mcpCommand,
  policyCommand,
  searchCommand,
  serveCommand,
  setupCommand,
} from "./commands/index.js";

const program = new Command()
  .name("mp")
  .description("moneypenny — local-first AI coding agent")
  .version("0.1.0")
  .option("-v, --verbose", "Enable debug output")
  .hook("preAction", (thisCommand) => {
    if (thisCommand.opts().verbose) {
      process.env.MP_VERBOSE = "1";
    }
  });

program.addCommand(chatCommand);
program.addCommand(searchCommand);
program.addCommand(indexCommand);
program.addCommand(inspectCommand);
program.addCommand(mcpCommand);
program.addCommand(configCommand);
program.addCommand(doctorCommand);
program.addCommand(setupCommand);
program.addCommand(policyCommand);
program.addCommand(eventsCommand);
program.addCommand(serveCommand);
program.addCommand(agentsCliCommand);
program.addCommand(cloudCommand);

program.parse();
