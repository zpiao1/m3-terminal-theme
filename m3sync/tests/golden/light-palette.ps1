# header (light)
$e = [char]27
$global:M3 = @{
    BranchEdge         = "$e[38;2;237;209;153m"
    BranchPill         = "$e[48;2;237;209;153m$e[38;2;79;61;18m"
    DirEdge            = "$e[38;2;109;242;225m"
    DirPill            = "$e[48;2;109;242;225m$e[38;2;0;89;80m"
    DirToBranch        = "$e[38;2;109;242;225m$e[48;2;237;209;153m"
    Err                = "$e[38;2;172;52;52m"
    Ok                 = "$e[38;2;0;107;97m"
}
$global:M3Syntax = @{
    Command            = "$e[38;2;0;107;97m"
    Comment            = "$e[38;2;72;132;132m"
    ContinuationPrompt = "$e[38;2;72;132;132m"
    Default            = "$e[38;2;0;57;57m"
    Emphasis           = "$e[1m$e[38;2;0;107;97m"
    Error              = "$e[38;2;172;52;52m"
    Keyword            = "$e[38;2;131;73;171m"
    Member             = "$e[38;2;0;104;160m"
    Number             = "$e[38;2;113;99;0m"
    Operator           = "$e[38;2;42;104;104m"
    Parameter          = "$e[38;2;125;88;0m"
    Selection          = "$e[48;2;252;223;166m$e[38;2;99;79;35m"
    String             = "$e[38;2;0;114;63m"
    Type               = "$e[38;2;0;111;108m"
    Variable           = "$e[38;2;113;92;47m"
}
