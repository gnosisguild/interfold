// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { expect } from 'chai'
import { deployCRISPProgram, deployMockInterfold, ethers } from './utils'

describe('CRISP Interfold binding', function () {
  it('binds once to the Interfold controller that registered the program', async function () {
    const [owner, other] = await ethers.getSigners()
    const mockInterfold = await deployMockInterfold()
    const program = await deployCRISPProgram({ mockInterfold, bindInterfold: false })
    const programAddress = await program.getAddress()
    const interfoldAddress = await mockInterfold.getAddress()

    expect(await program.owner()).to.equal(await owner.getAddress())
    expect(await program.interfold()).to.equal(ethers.ZeroAddress)

    await expect(program.connect(other).bindInterfold(interfoldAddress))
      .to.be.revertedWithCustomError(program, 'OwnableUnauthorizedAccount')
      .withArgs(await other.getAddress())
    await expect(program.bindInterfold(ethers.ZeroAddress)).to.be.revertedWithCustomError(program, 'InterfoldAddressZero')
    await expect(program.bindInterfold(await owner.getAddress())).to.be.revertedWithCustomError(program, 'InterfoldNotContract')
    await expect(program.bindInterfold(interfoldAddress)).to.be.revertedWithCustomError(program, 'ProgramNotRegistered')

    await (await mockInterfold.registerE3Program(programAddress)).wait()
    await expect(program.bindInterfold(interfoldAddress)).to.emit(program, 'InterfoldBound').withArgs(interfoldAddress)
    expect(await program.interfold()).to.equal(interfoldAddress)

    await expect(program.bindInterfold(interfoldAddress)).to.be.revertedWithCustomError(program, 'InterfoldAlreadyBound')
  })
})
