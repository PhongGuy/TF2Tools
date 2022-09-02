import {Injectable} from '@angular/core';
import {gameSound} from '../models/gameSound';
import {LogService} from './log.service';
import {ElectronService} from '../core/services';

@Injectable({
  providedIn: 'root'
})
export class GameSoundsService {

  constructor(
    private electron: ElectronService,
    private log: LogService
  ) {
  }

  getSounds(file: string): gameSound[] | null {
    if (this.checkIfFileExists(file)) {
      const data = this.electron.fs.readFileSync(file, {encoding: 'utf8', flag: 'r'});
      return this.getSoundsFromFile(data);
    }
    return null;
  }

  async getDefaultSounds(what: 'weaponSounds'): Promise<gameSound[] | null> {
    switch (what) {
      case 'weaponSounds':
        return fetch('https://raw.githubusercontent.com/powerlord/tf2-data/master/game_sounds/game_sounds_weapons.txt')
          .then(response => response.text()
            .then(text => this.getSoundsFromFile(text)
            ));
      default:
        return null;
    }
  }

  private checkIfFileExists(file: string): boolean {
    if (this.electron.fs.existsSync(file)) {
      this.log.info('READ', `Getting sounds from ${file}`);
      return true;
    }
    this.log.warn('READ', `File ${file} does not exist`);
    return false;
  }

  private getSoundsFromFile(file: string): gameSound[] {
    const sounds: gameSound[] = [];
    const lines = file.split('\n');
    const fileSounds = this.getFileSounds(lines);

    fileSounds.forEach(soundName => {
      const sound: gameSound = new gameSound();
      sound.name = soundName;
      let lineNumber = this.getLineNumber(lines, soundName);
      lineNumber++;
      if (lines[lineNumber].trim() === '{') {
        let findCloseTagNumber = 1;
        while (findCloseTagNumber !== 0) {
          lineNumber++;
          const line = this.getLine(lines, lineNumber);
          if (line.includes('}')) {
            findCloseTagNumber--;
          } else if (line.includes('{')) {
            findCloseTagNumber++;
          } else if (line !== '' && line !== '"rndwave"') {
            const name = this.getName(line);
            const value = this.getValue(line);
            if (findCloseTagNumber > 1) {
              sound.rndwave.push({value, lineNumber});
            } else {
              for (const key in sound) {
                if (name === key) {
                  sound[key] = {
                    value,
                    lineNumber
                  };
                }
              }
            }
          }
        }
      }
      sounds.push(sound);
    });
    return sounds;
  }


  private getValue(line: string) {
    return line.split('""')[1].replace(/"/g, '');
  }

  private getName(line: string) {
    return line.split('""')[0].replace(/"/g, '');
  }

  private getLine(lines: string[], lineNumber: number) {
    return lines[lineNumber]
      .replace(/ /g, '')
      .replace(/	/g, '')
      .trim();
  }

  private getLineNumber(lines: string[], weapon: string) {
    return lines.findIndex(x => x.includes(weapon) === true);
  }

  private getFileSounds(lines: string[]) {
    return lines
      .filter(x => x.startsWith('"'))
      .map(x => x.replace(/"/g, ''))
      .map(line => line.trim());
  }
}
