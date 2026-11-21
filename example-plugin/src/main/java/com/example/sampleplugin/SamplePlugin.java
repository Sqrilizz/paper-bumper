package com.example.sampleplugin;

import net.kyori.adventure.text.Component;
import org.bukkit.command.Command;
import org.bukkit.command.CommandExecutor;
import org.bukkit.command.CommandSender;
import org.bukkit.plugin.java.JavaPlugin;

public final class SamplePlugin extends JavaPlugin implements CommandExecutor {

    @Override
    public void onEnable() {
        getLogger().info("SamplePlugin has been enabled successfully");
    }

    @Override
    public void onDisable() {
        getLogger().info("SamplePlugin has been disabled successfully");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        sender.sendMessage(Component.text("SamplePlugin status: active"));
        return true;
    }
}
