import {Component, OnInit} from '@angular/core';
import {weaponSounds} from '../../../mock/weaponSounds';
import {AppComponent} from '../../../app.component';
import {LogService} from '../../../services/log.service';
import {gameSound} from '../../../models/gameSound';
import {GameSoundsService} from '../../../services/game-sounds.service';

@Component({
  selector: 'app-weapon-sounds',
  templateUrl: './weapon-sounds.component.html',
  styleUrls: ['./weapon-sounds.component.scss']
})
export class WeaponSoundsComponent implements OnInit {

  weaponSounds = weaponSounds;
  allWeaponSounds: gameSound[];
  defaultSounds: gameSound[];

  constructor(
    private app: AppComponent,
    private log: LogService,
    private gameSounds: GameSoundsService
  ) {
  }

  async ngOnInit() {
    this.app.update('weaponSounds');
    this.log.scope('WEAPON SOUNDS');
    this.allWeaponSounds = this.gameSounds.getSounds(this.app.weaponSounds);
    this.defaultSounds = await this.gameSounds.getDefaultSounds('weaponSounds');
    console.log(this.allWeaponSounds);
  }

}
