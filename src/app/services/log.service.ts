import { Injectable } from '@angular/core';
import * as moment from 'moment';
import { ElectronService } from '../core/services';
import { LogActions } from '../mock/logActions';

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
    private electron: ElectronService
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
   * Infos log service
   *
   * @param process
   * @param message
   */
  info(process: LogActions['process'], message: string): void {
    this.write('INFO', process, message);
  }

  /**
   * Errors log service
   *
   * @param process
   * @param message
   */
  error(process: LogActions['process'], message: string): void {
    this.write('ERROR', process, message);
  }

  /**
   * Cleanups log service
   *
   * This will remove old logs after 500 lines, so if error occur we can get log if people want to provide it.
   * This way we should only use about 50kb in log file size.
   */
  cleanup(): void {
    const logs = this.electron.fs.readFileSync(this.logPath, { encoding: 'utf8', flag: 'r' }).split('\n').reverse();
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
    const log = `${this.getDateTime()} (${this.scopeWhat}) [${level}] *${process}* > ${message}`;
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
