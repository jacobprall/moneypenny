class AddBackDebit < ActiveRecord::Migration[5.2]
  def change
    add_column :accounts, :debit, :boolean, null: false
  end
end
