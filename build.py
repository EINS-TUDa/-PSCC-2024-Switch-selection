# A script to manually compile the tool
# Change this value to False to make a debug build
BUILD_RELEASE = True
REMOVE_TARGET = True





error = False
warning = False

import os
import subprocess
import shutil
import re
import pathlib
import base64
try:
    from termcolor import cprint
except ModuleNotFoundError:
    print('''It seems like your Python environment doesn't have a termcolor module installed.
This module isn't neccessary but it's recommended for a better experience with this build script.

Shall it be installed now?
\t- yes, let's install it with (p)ip using 'pip install termcolor'.
\t- yes, let's install it with (c)onda using 'conda install termcolor'.
\t- (n)o, let's continue without it.
\t- (a)bort the execution of this script.''')
    command = input('Enter a symbol in brackets to choose from one of the options: ')
    if command == 'p':
        return_value = subprocess.run(['pip', 'install', 'termcolor'], capture_output=True, shell=True)
        if return_value.returncode == 0:
            from termcolor import cprint
        else:
            print('ERROR! Command \'pip install termcolor\' wasn\'t executed successfully. The exectution of this script will be aborted.')
            error = True
    elif command == 'c':
        return_value = subprocess.run(['conda', 'install', 'termcolor'], capture_output=True, shell=True)
        if return_value.returncode == 0:
            from termcolor import cprint
        else:
            print('ERROR! Command \'conda install termcolor\' wasn\'t executed successfully. The exectution of this script will be aborted.')
            error = True
    elif command == 'n':
        global cprint
        def cprint(message, _colour, end='\n'):
            print(message, end=end)
    elif command == 'a':
        error = True
    else:
        print(f'ERROR! Unknown command {command}. The execution of this script will be aborted.')
        error = True





width = os.get_terminal_size().columns
return_value = None

if not error:
    os.system('cls' if os.name == 'nt' else 'clear')
    print(' '.join(['*'] * (width // 2 + (width % 2))))
    print('* SWITCH SELECTION BUILD SCRIPT' + ' ' * (width - 32 - (width % 2 == 0)) + '*')
    print('*' + ' ' * (width - 2 - (width % 2 == 0)) + '*')
    print('*' + ' ' * (width - 34 - (width % 2 == 0)) + 'by Andrew Eliseev (JointPoints) *')
    print(' '.join(['*'] * (width // 2 + (width % 2))))
    print()

    if not BUILD_RELEASE:
        cprint('You are about to make a DEBUG build! If this is not your intention, open the source code', 'red')
        cprint('of this script and set BUILD_RELEASE to True!', 'red')
        print()

    # Check the presence of the Rust compiler
    print('1. Checking the availability of necessary programs')
    print('\trustc\t: ', end='')
    return_value = subprocess.run(['rustc'], capture_output=True, shell=True)
    if return_value.returncode == 0:
        cprint('OK', 'green')
    else:
        cprint('NOT FOUND', 'red')
        error = True
    print('\tcargo\t: ', end='')
    return_value = subprocess.run(['cargo'], capture_output=True, shell=True)
    if return_value.returncode == 0:
        cprint('OK', 'green')
    else:
        cprint('NOT FOUND', 'red')
        error = True
    print()
    if error:
        cprint('ERROR! ', 'red', end='')
        print('Rust compiler or Cargo build system not found.')
        print()

# Try to build the tool with Cargo
if not error:
    print('2. Building the tool (might take some time)')
    return_value = subprocess.run(['cargo', 'build', '-r'] if BUILD_RELEASE else ['cargo', 'build'], capture_output=True, shell=True)
    if return_value.returncode != 0:
        print()
        cprint('ERROR! ', 'red', end='')
        print('Compilation failed. The raw output of the Rust compiler follows.')
        print(return_value.stderr.decode())
        print()
        error = True
    else:
        cprint('\tOK', 'green')
        print()

# Replace the tool to our custom-made folder
if not error:
    print('3. Final arrangements')
    os.makedirs('./Switch selection/Graphs', exist_ok=True)
    try:
        subfolder_name = 'release' if BUILD_RELEASE else 'debug'
        shutil.copy(f'./target/{subfolder_name}/switch-selection' + '.exe' if os.name == 'nt' else '', './Switch selection')
    except shutil.SameFileError:
        pass
    if BUILD_RELEASE and REMOVE_TARGET:
        shutil.rmtree('./target')
    with open('./build.b64') as b64file:
        # Decode build.b64 into different files
        files = (('./Switch selection/Graphs/example1.gnbs', 'w', -1, 'utf-8'), ('./Switch selection/Graphs/example2.gnbs', 'w', -1, 'utf-8'), ('./Switch selection/Graphs/example3.gnbs', 'w', -1, 'utf-8'))
        for file in files:
            print(f'\t{file[0][19:]:<30}: ', end='')
            line = b64file.readline()
            with open(*file) as f:
                decoded = base64.b64decode(line)
                f.write(decoded.decode() if file[1] == 'w' else decoded)
            cprint('OK', 'green')
        # Try to find a CPLEX dynamic library
        cplex_lib_name = 'cplex*.' + 'dll' if os.name == 'nt' else 'so'
        print(f'\t{cplex_lib_name:<30}: ', end='')
        matching_env_vars = []
        for env_var_name in os.environ.keys():
            if re.match(r'CPLEX_STUDIO_DIR[0-9]+', env_var_name) != None:
                matching_env_vars.append(env_var_name)
        matching_env_vars = sorted(matching_env_vars)[::-1]
        for env_var_name in matching_env_vars:
            for cplex_dll_path in pathlib.Path(os.environ[env_var_name]).rglob(cplex_lib_name):
                shutil.copy(cplex_dll_path.resolve(), './Switch selection')
                cprint('OK', 'green')
                break
            else:
                continue
            break
        else:
            cprint('NOT FOUND', 'red')
            warning = True
    print()

# End
if not error:
    cprint('SUCCESS! ', 'green', end='')
    print('Build finished. You can find your compiled tool in the \'Switch selection\' folder.')
    print()
    if warning:
        cprint('WARNING! ', 'light_yellow', end='')
        print('CPLEX dynamic library file (cplex*.dll or cplex*.so) couldn\'t be automatically located on your system. Please, copy it manually into the \'Switch selection\' folder. Otherwise, you won\'t be able to use CPLEXSolver.')
        print()

print('Press <Enter> to finish...', end='')
input()
