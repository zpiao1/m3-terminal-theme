# header (dark)
$e = [char]27
$global:M3 = @{
    BranchEdge         = "$e[38;2;239;218;181m"
    BranchPill         = "$e[48;2;239;218;181m$e[38;2;80;67;40m"
    DirEdge            = "$e[38;2;0;109;99m"
    DirPill            = "$e[48;2;0;109;99m$e[38;2;241;255;251m"
    DirToBranch        = "$e[38;2;0;109;99m$e[48;2;239;218;181m"
    Err                = "$e[38;2;255;113;108m"
    Ok                 = "$e[38;2;152;236;223m"
}
$global:M3Syntax = @{
    Command            = "$e[38;2;152;236;223m"
    Comment            = "$e[38;2;83;125;124m"
    ContinuationPrompt = "$e[38;2;83;125;124m"
    Default            = "$e[38;2;194;238;237m"
    Emphasis           = "$e[1m$e[38;2;152;236;223m"
    Error              = "$e[38;2;255;113;108m"
    Keyword            = "$e[38;2;224;174;255m"
    Member             = "$e[38;2;136;199;255m"
    Number             = "$e[38;2;219;193;0m"
    Operator           = "$e[38;2;136;179;178m"
    Parameter          = "$e[38;2;255;201;103m"
    Selection          = "$e[48;2;47;36;12m$e[38;2;180;162;128m"
    String             = "$e[38;2;97;215;143m"
    Type               = "$e[38;2;70;213;207m"
    Variable           = "$e[38;2;216;196;160m"
}
