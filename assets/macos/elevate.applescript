set executablePath to system attribute "BOUCHONNEUR_EXECUTABLE"
set operationName to system attribute "BOUCHONNEUR_OPERATION"
set sourcePath to system attribute "BOUCHONNEUR_SOURCE"
set targetPath to system attribute "BOUCHONNEUR_TARGET"

if operationName is "deploy" then
    set commandText to quoted form of executablePath & " --privileged-deploy " & quoted form of sourcePath & " " & quoted form of targetPath
else if operationName is "delete" then
    set commandText to quoted form of executablePath & " --privileged-delete " & quoted form of targetPath
else if operationName is "restore" then
    set commandText to quoted form of executablePath & " --privileged-restore " & quoted form of sourcePath & " " & quoted form of targetPath
else
    error "Opération privilégiée inconnue." number 64
end if

do shell script commandText with administrator privileges
