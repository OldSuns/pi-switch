use super::keys::Command;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Language {
    #[default]
    English,
    Chinese,
}

impl Language {
    pub(super) fn from_code(value: &str) -> Self {
        if value == "zh-CN" {
            Self::Chinese
        } else {
            Self::English
        }
    }

    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
        }
    }

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "中文",
        }
    }

    pub(super) const fn next(self) -> Self {
        match self {
            Self::English => Self::Chinese,
            Self::Chinese => Self::English,
        }
    }

    pub(super) const fn pick(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::Chinese => chinese,
        }
    }

    pub(super) const fn command_label(self, command: Command) -> &'static str {
        match command {
            Command::Quit => self.pick("quit", "退出"),
            Command::Help => self.pick("help", "帮助"),
            Command::Filter => self.pick("filter", "筛选"),
            Command::New => self.pick("new", "新建"),
            Command::Edit => self.pick("edit", "编辑"),
            Command::Delete => self.pick("delete", "删除"),
            Command::Copy => self.pick("copy", "复制"),
            Command::Import => self.pick("import", "导入"),
            Command::SetDefault => self.pick("default", "默认"),
            Command::Backups => self.pick("backups", "备份"),
            Command::Doctor => self.pick("check", "检查"),
            Command::Reload => self.pick("reload", "重载"),
        }
    }

    pub(super) const fn command_help(self, command: Command) -> &'static str {
        match command {
            Command::Quit => self.pick("quit", "退出"),
            Command::Help => self.pick("show all shortcuts", "显示全部快捷键"),
            Command::Filter => self.pick("filter providers", "筛选提供商"),
            Command::New => self.pick("new focused item", "新建当前类型"),
            Command::Edit => self.pick("edit focused item", "编辑当前项目"),
            Command::Delete => self.pick("delete focused item", "删除当前项目"),
            Command::Copy => self.pick("copy focused item", "复制当前项目"),
            Command::Import => self.pick("import models from provider", "从提供商导入模型"),
            Command::SetDefault => self.pick("set selected model as default", "设为默认模型"),
            Command::Backups => self.pick("browse backups", "浏览备份"),
            Command::Doctor => self.pick("validate configuration", "验证配置"),
            Command::Reload => self.pick("reload configuration from disk", "从磁盘重载配置"),
        }
    }
}
