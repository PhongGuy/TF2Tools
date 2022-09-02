import {Injectable} from '@angular/core';
import * as moment from 'moment';
import {ElectronService} from '../core/services';
import {LogActions} from '../mock/logActions';
import {SnackService} from './snack.service';

/**
 * Log Service
 */
@Injectable({
  providedIn: 'root'
})
export class LogService {

  /**
   * Log path of log service
   */
  private logPath: string;
  /**
   * Scope what of log service
   */
  private scopeWhat = 'undefined';

  /**
   * Creates an instance of log service.
   *
   * @param electron
   */
  constructor(
    private electron: ElectronService,
    private snack: SnackService
  ) {
    this.logPath = this.electron.appData('TF2Tools\\log.log');
    this.electron.fs.ensureFile(this.logPath);
  }

  /**
   * Scopes log service
   *
   * @param what
   */
  scope(what: LogActions['what']): void {
    this.scopeWhat = what;
  }

  /**
   * Log info
   *
   * @param process
   * @param message
   */
  info(process: LogActions['process'], message: string): void {
    this.write('INFO', process, message);
  }

  /**
   * Log warning
   *
   * @param process
   * @param message
   */
  warn(process: LogActions['process'], message: string): void {
    this.write('WARN', process, message);
  }

  /**
   * Log error
   *
   * @param process
   * @param message
   */
  error(process: LogActions['process'], message: string): void {
    this.write('ERROR', process, message);
    this.snack.show(`We encountered an error. See logs in settings for more information`);
  }

  /**
   * Cleanups log service
   *
   * This will remove old logs after 500 lines, so if error occur we can get log if people want to provide it.
   * This way we should only use about 50kb in log file size.
   */
  cleanup(): void {
    const logs = this.electron.fs.readFileSync(this.logPath, {encoding: 'utf8', flag: 'r'}).split('\n').reverse();
    const sliced = logs.slice(0, 501);
    this.electron.fs.writeFileSync(this.logPath, sliced.reverse().join('\n'));
  }

  /**
   * Writes log service
   *
   * @param level
   * @param process
   * @param message
   */
  private write(level: LogActions['level'], process: LogActions['process'], message: string): void {
    const dateTime = this.getDateTime();
    const levelText = `[${level}]` + '       '.slice(level.length + 2);
    const processText = `*${process}*` + '        '.slice(process.length + 2);
    const scopeText = `(${this.scopeWhat})` + '               '.slice(this.scopeWhat.length + 2);

    const log = `${dateTime} ${levelText} ${scopeText} ${processText} ${message}`;

    this.electron.fs.appendFile(this.logPath, `${log}\n`).then(() => {
      console.log(log);
    });
  }

  /**
   * Gets date time
   *
   * @returns date time
   */
  private getDateTime(): string {
    return moment().format('DD-MM-YYYY HH:mm:ss');
  }
}
