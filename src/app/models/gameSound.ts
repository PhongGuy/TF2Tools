import {ValueLineNumber} from './valueLineNumber';

export class GameSound {
  name: string = null;
  channel: ValueLineNumber = null;
  soundlevel: ValueLineNumber = null;
  volume: ValueLineNumber = null;
  wave: ValueLineNumber = null;
  pitch: ValueLineNumber = null;
  rndwave: ValueLineNumber[] = [];
}
